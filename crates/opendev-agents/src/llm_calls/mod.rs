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

    /// Clean and normalize messages before sending to the LLM API.
    ///
    /// Four phases applied in order:
    /// 1. Filter out `Internal`-class messages and strip `_`-prefixed metadata keys
    /// 2. Remove whitespace-only messages (preserving tool results and tool-call-only assistants)
    /// 3. Merge consecutive same-role messages (user or assistant)
    /// 4. Remove orphaned tool results (no matching `tool_call_id` in any assistant message)
    pub fn clean_messages(messages: &[Value]) -> Vec<Value> {
        let filtered = Self::filter_internal_and_strip(messages);
        let filtered = Self::filter_whitespace_only(filtered);
        let merged = Self::merge_consecutive(filtered);
        Self::remove_orphaned_tool_results(merged)
    }

    /// Phase 1: Filter out Internal-class messages and strip `_`-prefixed keys.
    fn filter_internal_and_strip(messages: &[Value]) -> Vec<Value> {
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

    /// Phase 2: Remove messages with empty or whitespace-only content.
    ///
    /// Preserves:
    /// - `role: "tool"` messages (structurally required even if empty)
    /// - `role: "assistant"` messages with non-empty `tool_calls` (tool-only responses)
    fn filter_whitespace_only(messages: Vec<Value>) -> Vec<Value> {
        messages
            .into_iter()
            .filter(|msg| {
                let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
                if role == "tool" {
                    return true;
                }
                if role == "assistant"
                    && let Some(tc) = msg.get("tool_calls").and_then(|v| v.as_array())
                    && !tc.is_empty()
                {
                    return true;
                }
                match msg.get("content").and_then(|v| v.as_str()) {
                    Some(s) => !s.trim().is_empty(),
                    None => {
                        // Keep non-object values (backwards compat) and messages without content
                        !msg.is_object()
                    }
                }
            })
            .collect()
    }

    /// Phase 3: Merge consecutive messages with the same role.
    ///
    /// Only merges `user` and `assistant` roles. Tool messages are never merged
    /// (each has a unique `tool_call_id`). System messages pass through individually.
    fn merge_consecutive(messages: Vec<Value>) -> Vec<Value> {
        let mut result: Vec<Value> = Vec::with_capacity(messages.len());

        for msg in messages {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");

            if role != "user" && role != "assistant" {
                result.push(msg);
                continue;
            }

            let should_merge = result
                .last()
                .and_then(|prev| prev.get("role").and_then(|v| v.as_str()))
                .is_some_and(|prev_role| prev_role == role);

            if should_merge {
                let prev = result.last_mut().unwrap();
                Self::merge_into(prev, &msg);
            } else {
                result.push(msg);
            }
        }

        result
    }

    /// Merge `source` message content and tool_calls into `target`.
    fn merge_into(target: &mut Value, source: &Value) {
        let target_content = target
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let source_content = source
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let merged_content = match (target_content.is_empty(), source_content.is_empty()) {
            (_, true) => target_content.to_string(),
            (true, _) => source_content.to_string(),
            _ => format!("{target_content}\n\n{source_content}"),
        };
        target["content"] = Value::String(merged_content);

        // Merge tool_calls arrays (relevant for assistant messages)
        if let Some(source_tc) = source.get("tool_calls").and_then(|v| v.as_array())
            && !source_tc.is_empty()
        {
            let mut combined = target
                .get("tool_calls")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            combined.extend(source_tc.iter().cloned());
            target["tool_calls"] = Value::Array(combined);
        }
    }

    /// Phase 4: Remove tool result messages whose `tool_call_id` has no matching
    /// entry in any assistant message's `tool_calls` array.
    fn remove_orphaned_tool_results(messages: Vec<Value>) -> Vec<Value> {
        let valid_ids: std::collections::HashSet<String> = messages
            .iter()
            .filter(|m| m.get("role").and_then(|v| v.as_str()) == Some("assistant"))
            .filter_map(|m| m.get("tool_calls").and_then(|v| v.as_array()))
            .flatten()
            .filter_map(|tc| tc.get("id").and_then(|v| v.as_str()).map(String::from))
            .collect();

        messages
            .into_iter()
            .filter(|msg| {
                if msg.get("role").and_then(|v| v.as_str()) != Some("tool") {
                    return true;
                }
                msg.get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| valid_ids.contains(id as &str))
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
