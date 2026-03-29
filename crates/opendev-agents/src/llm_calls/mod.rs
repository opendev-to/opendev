//! LLM API call methods.
//!
//! Provides `LlmCaller` with methods for action calls (with tools),
//! thinking calls (no tools, reasoning only), and response parsing.

mod model_detection;

use model_detection::{insert_max_tokens, insert_temperature};
pub use model_detection::{is_reasoning_model, supports_temperature, uses_max_completion_tokens};

use serde_json::Value;
use tracing::{debug, warn};

use crate::response::ResponseCleaner;
use crate::traits::LlmResponse;

/// Configuration for an LLM call.
#[derive(Debug, Clone)]
pub struct LlmCallConfig {
    /// Model identifier (e.g. "gpt-4o", "claude-3-opus").
    pub model: String,
    /// Temperature for sampling.
    pub temperature: Option<f64>,
    /// Maximum tokens to generate.
    pub max_tokens: Option<u64>,
    /// Reasoning effort level ("low", "medium", "high", or "none").
    pub reasoning_effort: Option<String>,
}

/// Handles different types of LLM calls (normal, compact).
///
/// Uses composition instead of Python's mixin pattern. Holds a `ResponseCleaner`
/// and call configuration, producing structured `LlmResponse` values.
#[derive(Debug, Clone)]
pub struct LlmCaller {
    cleaner: ResponseCleaner,
    /// Primary model config.
    pub config: LlmCallConfig,
}

impl LlmCaller {
    /// Create a new LLM caller with the given primary model configuration.
    pub fn new(config: LlmCallConfig) -> Self {
        Self {
            cleaner: ResponseCleaner::new(),
            config,
        }
    }

    /// Strip internal `_`-prefixed keys and filter out `Internal`-class messages
    /// before API calls.
    pub fn clean_messages(messages: &[Value]) -> Vec<Value> {
        messages
            .iter()
            .filter(|msg| msg.get("_msg_class").and_then(|v| v.as_str()) != Some("internal"))
            .map(|msg| {
                if let Some(obj) = msg.as_object() {
                    if obj.keys().any(|k| k.starts_with('_')) {
                        let cleaned: serde_json::Map<String, Value> = obj
                            .iter()
                            .filter(|(k, _)| !k.starts_with('_'))
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        Value::Object(cleaned)
                    } else {
                        msg.clone()
                    }
                } else {
                    msg.clone()
                }
            })
            .collect()
    }

    /// Build an LLM payload for an action call (with tools).
    pub fn build_action_payload(&self, messages: &[Value], tool_schemas: &[Value]) -> Value {
        let mut payload = serde_json::json!({
            "model": self.config.model,
            "messages": Self::clean_messages(messages),
            "tools": tool_schemas,
            "tool_choice": "auto",
        });

        if let Some(temp) = self.config.temperature {
            insert_temperature(&mut payload, &self.config.model, temp);
        }
        if let Some(max) = self.config.max_tokens {
            insert_max_tokens(&mut payload, &self.config.model, max);
        }

        // Inject reasoning effort for adapters to consume
        if let Some(ref effort) = self.config.reasoning_effort {
            payload["_reasoning_effort"] = serde_json::json!(effort);
        }

        payload
    }

    /// Parse an action response (with potential tool calls) into an `LlmResponse`.
    pub fn parse_action_response(&self, body: &Value) -> LlmResponse {
        let choices = match body.get("choices").and_then(|c| c.as_array()) {
            Some(c) if !c.is_empty() => c,
            _ => {
                warn!("No choices in LLM response");
                return LlmResponse::fail("No choices in response");
            }
        };

        let choice = &choices[0];
        let message = match choice.get("message") {
            Some(m) => m,
            None => {
                warn!("No message in choice");
                return LlmResponse::fail("No message in response choice");
            }
        };

        let raw_content = message.get("content").and_then(|c| c.as_str());
        let cleaned_content = self.cleaner.clean(raw_content);
        let reasoning_content = message
            .get("reasoning_content")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());

        debug!(
            has_content = raw_content.is_some(),
            has_tool_calls = message.get("tool_calls").is_some(),
            "Parsed action response"
        );

        let finish_reason = choice
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .map(|s| s.to_string());

        let mut resp = LlmResponse::ok(cleaned_content, message.clone());
        resp.usage = body.get("usage").cloned();
        resp.reasoning_content = reasoning_content;
        resp.finish_reason = finish_reason;
        resp
    }
}

#[cfg(test)]
mod tests;
