//! Mistral AI adapter.
//!
//! Mistral's API is OpenAI-compatible (Chat Completions format) but with
//! minor differences in tool calling structure. This adapter handles:
//! - Passing requests mostly unchanged (OpenAI Chat Completions format)
//! - Normalizing tool call responses (Mistral may omit `type` field)
//! - Endpoint: `https://api.mistral.ai/v1/chat/completions`

use serde_json::{Value, json};

const DEFAULT_API_URL: &str = "https://api.mistral.ai/v1/chat/completions";

/// Adapter for the Mistral AI Chat Completions API.
///
/// Mistral uses an OpenAI-compatible format but with slight differences
/// in how tool calls are structured (e.g., `type` field may be absent
/// in tool call responses, and `arguments` may be a JSON object instead
/// of a string).
#[derive(Debug, Clone)]
pub struct MistralAdapter {
    api_url: String,
}

impl MistralAdapter {
    /// Create a new Mistral adapter with the default API URL.
    pub fn new() -> Self {
        Self {
            api_url: DEFAULT_API_URL.to_string(),
        }
    }

    /// Create with a custom API URL.
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            api_url: url.into(),
        }
    }

    /// Normalize tool calls in the response.
    ///
    /// Mistral may return tool calls with:
    /// - Missing `type` field (should be "function")
    /// - `arguments` as a JSON object instead of a string
    fn normalize_tool_calls(response: &mut Value) {
        if let Some(choices) = response.get_mut("choices").and_then(|c| c.as_array_mut()) {
            for choice in choices.iter_mut() {
                if let Some(tool_calls) = choice
                    .get_mut("message")
                    .and_then(|m| m.get_mut("tool_calls"))
                    .and_then(|tc| tc.as_array_mut())
                {
                    for tc in tool_calls.iter_mut() {
                        // Ensure type field is present
                        if tc.get("type").is_none() {
                            tc["type"] = json!("function");
                        }

                        // If arguments is an object, serialize it to a string
                        if let Some(func) = tc.get_mut("function")
                            && let Some(args) = func.get("arguments")
                            && (args.is_object() || args.is_array())
                        {
                            let args_str = serde_json::to_string(args).unwrap_or_default();
                            func["arguments"] = Value::String(args_str);
                        }
                    }
                }
            }
        }
    }

    /// Remove unsupported parameters from the request payload.
    ///
    /// Mistral does not support some OpenAI-specific parameters.
    fn clean_request(payload: &mut Value) {
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("logprobs");
            obj.remove("top_logprobs");
            obj.remove("n");
            obj.remove("seed");
        }
    }
}

impl Default for MistralAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for MistralAdapter {
    fn provider_name(&self) -> &str {
        "mistral"
    }

    fn convert_request(&self, mut payload: Value) -> Value {
        Self::clean_request(&mut payload);
        // Strip internal reasoning effort field
        payload
            .as_object_mut()
            .map(|obj| obj.remove("_reasoning_effort"));
        payload
    }

    fn convert_response(&self, mut response: Value) -> Value {
        Self::normalize_tool_calls(&mut response);
        response
    }

    fn api_url(&self) -> &str {
        &self.api_url
    }
}

#[cfg(test)]
#[path = "mistral_tests.rs"]
mod tests;
