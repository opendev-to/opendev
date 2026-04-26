//! Anthropic-specific adapter.
//!
//! Handles Anthropic API differences:
//! - Messages API format (system as top-level field, not in messages)
//! - `anthropic-version` header
//! - Prompt caching via `cache_control` blocks
//! - Image blocks using Anthropic's native `source` format

mod request;
pub(crate) mod response;

use serde_json::{Value, json};

const DEFAULT_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Adapter for the Anthropic Messages API.
#[derive(Debug, Clone)]
pub struct AnthropicAdapter {
    api_url: String,
    enable_caching: bool,
}

impl AnthropicAdapter {
    /// Create a new Anthropic adapter.
    pub fn new() -> Self {
        Self {
            api_url: DEFAULT_API_URL.to_string(),
            enable_caching: true,
        }
    }

    /// Create with a custom API URL.
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            api_url: url.into(),
            enable_caching: true,
        }
    }

    /// Enable or disable prompt caching.
    pub fn with_caching(mut self, enable: bool) -> Self {
        self.enable_caching = enable;
        self
    }
}

/// Check if a model supports extended thinking (Claude 3.7+).
fn supports_thinking(model: &str) -> bool {
    let m = model.to_lowercase();
    m.starts_with("claude-3-7")
        || m.starts_with("claude-3.7")
        || m.starts_with("claude-4")
        || m.starts_with("claude-opus")
        || m.starts_with("claude-sonnet-4")
        || m.starts_with("claude-sonnet-5")
}

/// Check if a model supports adaptive thinking (Claude 4.6+ only).
/// Adaptive thinking uses `type: "adaptive"` instead of `type: "enabled"`,
/// letting the model decide how much to think rather than requiring a fixed budget.
fn supports_adaptive_thinking(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("opus-4-6")
        || m.contains("opus-4.6")
        || m.contains("sonnet-4-6")
        || m.contains("sonnet-4.6")
}

impl Default for AnthropicAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for AnthropicAdapter {
    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn convert_request(&self, mut payload: Value) -> Value {
        // Extract and handle reasoning effort before other conversions
        let reasoning_effort = payload
            .as_object_mut()
            .and_then(|obj| obj.remove("_reasoning_effort"))
            .and_then(|v| v.as_str().map(String::from));

        Self::extract_system(&mut payload);
        Self::convert_image_blocks(&mut payload);
        Self::convert_tools(&mut payload);
        Self::convert_tool_messages(&mut payload);
        Self::ensure_max_tokens(&mut payload);

        // Configure extended thinking if requested and supported
        let model = payload
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(ref effort) = reasoning_effort
            && effort != "none"
            && supports_thinking(&model)
        {
            if supports_adaptive_thinking(&model) {
                // Claude 4.6+ adaptive thinking: the model chooses its own budget.
                // The Messages API rejects `thinking.adaptive.budget_tokens` with
                // "Extra inputs are not permitted", so we never attach a cap here —
                // the reasoning_effort signal is carried by max_tokens / temperature
                // alone for adaptive-capable models.
                payload["thinking"] = json!({ "type": "adaptive" });
            } else {
                // Legacy models (3.7, 4.0) use fixed budget thinking
                let budget_tokens: u64 = match effort.as_str() {
                    "low" => 4000,
                    "medium" => 16000,
                    "high" => 31999,
                    _ => 16000,
                };
                payload["thinking"] = json!({
                    "type": "enabled",
                    "budget_tokens": budget_tokens
                });
                // Ensure max_tokens >= budget_tokens + 1024
                let current_max = payload
                    .get("max_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(16384);
                let min_max = budget_tokens + 1024;
                if current_max < min_max {
                    payload["max_tokens"] = json!(min_max);
                }
            }
            // Anthropic requires temperature=1 for extended thinking
            payload["temperature"] = json!(1);
        }

        if self.enable_caching {
            Self::add_cache_control(&mut payload);
        }

        // Remove unsupported fields
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("n");
            obj.remove("frequency_penalty");
            obj.remove("presence_penalty");
            obj.remove("logprobs");
        }

        payload
    }

    fn convert_response(&self, response: Value) -> Value {
        Self::response_to_chat_completions(response)
    }

    fn api_url(&self) -> &str {
        &self.api_url
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn enable_streaming(&self, payload: &mut Value) {
        payload["stream"] = json!(true);
    }

    fn parse_stream_event(
        &self,
        event_type: &str,
        data: &Value,
    ) -> Option<crate::streaming::StreamEvent> {
        self.parse_stream_event_impl(event_type, data)
    }

    fn extra_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![("anthropic-version".into(), ANTHROPIC_VERSION.into())];
        // Build beta features list
        let mut beta_features = Vec::new();
        if self.enable_caching {
            beta_features.push("prompt-caching-2024-07-31");
        }
        // Always include thinking beta — harmless when thinking isn't enabled
        beta_features.push("interleaved-thinking-2025-05-14");
        if !beta_features.is_empty() {
            headers.push(("anthropic-beta".into(), beta_features.join(",")));
        }
        headers
    }
}

#[cfg(test)]
mod tests;
