//! LLM API call methods.
//!
//! Mirrors `opendev/core/agents/main_agent/llm_calls.py`.
//! Provides `LlmCaller` with methods for normal, thinking, critique, and compact calls.

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
}

/// Handles different types of LLM calls (normal, thinking, critique, compact).
///
/// Uses composition instead of Python's mixin pattern. Holds a `ResponseCleaner`
/// and call configuration, producing structured `LlmResponse` values.
#[derive(Debug, Clone)]
pub struct LlmCaller {
    cleaner: ResponseCleaner,
    /// Primary model config.
    pub config: LlmCallConfig,
    /// Optional thinking model config (falls back to primary).
    pub thinking_config: Option<LlmCallConfig>,
    /// Optional critique model config (falls back to thinking, then primary).
    pub critique_config: Option<LlmCallConfig>,
}

impl LlmCaller {
    /// Create a new LLM caller with the given primary model configuration.
    pub fn new(config: LlmCallConfig) -> Self {
        Self {
            cleaner: ResponseCleaner::new(),
            config,
            thinking_config: None,
            critique_config: None,
        }
    }

    /// Set the thinking model configuration.
    pub fn with_thinking_config(mut self, config: LlmCallConfig) -> Self {
        self.thinking_config = Some(config);
        self
    }

    /// Set the critique model configuration.
    pub fn with_critique_config(mut self, config: LlmCallConfig) -> Self {
        self.critique_config = Some(config);
        self
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

    /// Build an LLM payload for a thinking call (no tools, pure reasoning).
    pub fn build_thinking_payload(
        &self,
        messages: &[Value],
        thinking_system_prompt: Option<&str>,
        analysis_prompt: Option<&str>,
    ) -> Value {
        let cfg = self.thinking_config.as_ref().unwrap_or(&self.config);

        let cleaned: Vec<Value> = messages
            .iter()
            .filter(|msg| {
                if msg.get("_thinking").and_then(|v| v.as_bool()) == Some(true) {
                    return false;
                }
                match msg.get("_msg_class").and_then(|v| v.as_str()) {
                    Some("nudge") | Some("internal") => false,
                    Some(_) | None => !msg.get("_nudge").and_then(|v| v.as_bool()).unwrap_or(false),
                }
            })
            .cloned()
            .collect();
        let mut cleaned = Self::clean_messages(&cleaned);

        if let Some(sys_prompt) = thinking_system_prompt {
            let has_system = cleaned
                .first()
                .and_then(|m| m.get("role"))
                .and_then(|r| r.as_str())
                == Some("system");

            if has_system {
                cleaned[0] = serde_json::json!({
                    "role": "system",
                    "content": sys_prompt,
                });
            } else {
                cleaned.insert(
                    0,
                    serde_json::json!({
                        "role": "system",
                        "content": sys_prompt,
                    }),
                );
            }
        }

        if let Some(analysis) = analysis_prompt {
            cleaned.push(serde_json::json!({
                "role": "user",
                "content": analysis,
            }));
        }

        let mut payload = serde_json::json!({
            "model": cfg.model,
            "messages": cleaned,
        });

        if let Some(temp) = cfg.temperature {
            insert_temperature(&mut payload, &cfg.model, temp);
        }
        if let Some(max) = cfg.max_tokens {
            insert_max_tokens(&mut payload, &cfg.model, max);
        }

        payload
    }

    /// Build an LLM payload for a thinking refinement call.
    pub fn build_refinement_payload(
        &self,
        thinking_system_prompt: &str,
        original_trace: &str,
        critique: &str,
    ) -> Value {
        let cfg = self.thinking_config.as_ref().unwrap_or(&self.config);

        let messages = vec![
            serde_json::json!({
                "role": "system",
                "content": thinking_system_prompt,
            }),
            serde_json::json!({
                "role": "user",
                "content": format!(
                    "Here is your previous thinking trace:\n\n\
                     <original_trace>\n{original_trace}\n</original_trace>\n\n\
                     Here is the critique of your reasoning:\n\n\
                     <critique>\n{critique}\n</critique>\n\n\
                     Refine your thinking trace to address the critique. \
                     Produce an improved, concise action plan for the next step."
                ),
            }),
        ];

        let max_tokens = cfg.max_tokens.map(|m| m.min(4096)).unwrap_or(4096);

        let mut payload = serde_json::json!({
            "model": cfg.model,
            "messages": messages,
        });

        insert_max_tokens(&mut payload, &cfg.model, max_tokens);

        if let Some(temp) = cfg.temperature {
            insert_temperature(&mut payload, &cfg.model, temp);
        }

        payload
    }

    /// Build an LLM payload for a critique call.
    pub fn build_critique_payload(
        &self,
        thinking_trace: &str,
        critique_system_prompt: &str,
    ) -> Value {
        let cfg = self
            .critique_config
            .as_ref()
            .or(self.thinking_config.as_ref())
            .unwrap_or(&self.config);

        let max_tokens = cfg.max_tokens.map(|m| m.min(2048)).unwrap_or(2048);

        let messages = vec![
            serde_json::json!({"role": "system", "content": critique_system_prompt}),
            serde_json::json!({
                "role": "user",
                "content": format!("Please critique the following thinking trace:\n\n{thinking_trace}")
            }),
        ];

        let mut payload = serde_json::json!({
            "model": cfg.model,
            "messages": messages,
        });

        insert_max_tokens(&mut payload, &cfg.model, max_tokens);

        if let Some(temp) = cfg.temperature {
            insert_temperature(&mut payload, &cfg.model, temp);
        }

        payload
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

        payload
    }

    /// Parse a thinking response (no tools) into an `LlmResponse`.
    pub fn parse_thinking_response(&self, body: &Value) -> LlmResponse {
        self.parse_text_only_response(body)
    }

    /// Parse a critique response into an `LlmResponse`.
    pub fn parse_critique_response(&self, body: &Value) -> LlmResponse {
        self.parse_text_only_response(body)
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

        let mut resp = LlmResponse::ok(cleaned_content, message.clone());
        resp.usage = body.get("usage").cloned();
        resp.reasoning_content = reasoning_content;
        resp
    }

    /// Internal: parse a text-only response (thinking or critique).
    fn parse_text_only_response(&self, body: &Value) -> LlmResponse {
        let choices = match body.get("choices").and_then(|c| c.as_array()) {
            Some(c) if !c.is_empty() => c,
            _ => return LlmResponse::fail("No choices in response"),
        };

        let message = match choices[0].get("message") {
            Some(m) => m,
            None => return LlmResponse::fail("No message in response choice"),
        };

        let raw_content = message.get("content").and_then(|c| c.as_str());
        let cleaned_content = self.cleaner.clean(raw_content);

        LlmResponse {
            success: true,
            content: cleaned_content,
            tool_calls: None,
            message: Some(message.clone()),
            error: None,
            interrupted: false,
            usage: body.get("usage").cloned(),
            reasoning_content: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_caller() -> LlmCaller {
        LlmCaller::new(LlmCallConfig {
            model: "gpt-4o".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(4096),
        })
    }

    #[test]
    fn test_clean_messages_strips_underscore_keys() {
        let messages = vec![
            serde_json::json!({"role": "user", "content": "hello", "_internal": true}),
            serde_json::json!({"role": "assistant", "content": "world"}),
        ];
        let cleaned = LlmCaller::clean_messages(&messages);
        assert!(cleaned[0].get("_internal").is_none());
        assert_eq!(cleaned[0]["role"], "user");
        assert_eq!(cleaned[1]["role"], "assistant");
    }

    #[test]
    fn test_clean_messages_preserves_non_object() {
        let messages = vec![serde_json::json!("string_value")];
        let cleaned = LlmCaller::clean_messages(&messages);
        assert_eq!(cleaned[0], "string_value");
    }

    #[test]
    fn test_build_thinking_payload() {
        let caller = make_caller();
        let messages = vec![serde_json::json!({"role": "user", "content": "think"})];
        let payload = caller.build_thinking_payload(&messages, None, None);
        assert_eq!(payload["model"], "gpt-4o");
        assert!(payload.get("tools").is_none());
        assert!(payload["messages"].as_array().unwrap().len() == 1);
    }

    #[test]
    fn test_build_thinking_payload_with_thinking_model() {
        let caller = make_caller().with_thinking_config(LlmCallConfig {
            model: "o1-preview".to_string(),
            temperature: None,
            max_tokens: Some(8192),
        });
        let messages = vec![serde_json::json!({"role": "user", "content": "think"})];
        let payload = caller.build_thinking_payload(&messages, None, None);
        assert_eq!(payload["model"], "o1-preview");
        assert_eq!(payload["max_completion_tokens"], 8192);
        assert!(payload.get("max_tokens").is_none());
        assert!(payload.get("temperature").is_none());
    }

    #[test]
    fn test_build_thinking_payload_filters_legacy_nudges() {
        let caller = make_caller();
        let messages = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({"role": "user", "content": "nudge msg", "_nudge": true}),
            serde_json::json!({"role": "assistant", "content": "reply"}),
        ];
        let payload = caller.build_thinking_payload(&messages, None, None);
        let msgs = payload["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["content"], "hello");
        assert_eq!(msgs[1]["content"], "reply");
    }

    #[test]
    fn test_thinking_payload_keeps_directives() {
        let caller = make_caller();
        let messages = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({"role": "user", "content": "[SYSTEM] error context", "_msg_class": "directive"}),
            serde_json::json!({"role": "assistant", "content": "reply"}),
        ];
        let payload = caller.build_thinking_payload(&messages, None, None);
        let msgs = payload["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[1]["content"], "[SYSTEM] error context");
        assert!(msgs[1].get("_msg_class").is_none());
    }

    #[test]
    fn test_thinking_payload_strips_nudge_class() {
        let caller = make_caller();
        let messages = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({"role": "user", "content": "[SYSTEM] todo nudge", "_msg_class": "nudge"}),
            serde_json::json!({"role": "assistant", "content": "reply"}),
        ];
        let payload = caller.build_thinking_payload(&messages, None, None);
        let msgs = payload["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn test_thinking_payload_strips_internal_class() {
        let caller = make_caller();
        let messages = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({"role": "user", "content": "[SYSTEM] debug info", "_msg_class": "internal"}),
            serde_json::json!({"role": "assistant", "content": "reply"}),
        ];
        let payload = caller.build_thinking_payload(&messages, None, None);
        let msgs = payload["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn test_clean_messages_strips_internal() {
        let messages = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({"role": "user", "content": "[SYSTEM] debug", "_msg_class": "internal"}),
            serde_json::json!({"role": "user", "content": "[SYSTEM] error", "_msg_class": "directive"}),
            serde_json::json!({"role": "user", "content": "[SYSTEM] nudge", "_msg_class": "nudge"}),
        ];
        let cleaned = LlmCaller::clean_messages(&messages);
        assert_eq!(cleaned.len(), 3);
        assert_eq!(cleaned[0]["content"], "hello");
        assert_eq!(cleaned[1]["content"], "[SYSTEM] error");
        assert_eq!(cleaned[2]["content"], "[SYSTEM] nudge");
        assert!(cleaned[1].get("_msg_class").is_none());
        assert!(cleaned[2].get("_msg_class").is_none());
    }

    #[test]
    fn test_build_thinking_payload_swaps_system_prompt() {
        let caller = make_caller();
        let messages = vec![
            serde_json::json!({"role": "system", "content": "original system"}),
            serde_json::json!({"role": "user", "content": "hello"}),
        ];
        let payload = caller.build_thinking_payload(&messages, Some("thinking system"), None);
        let msgs = payload["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["content"], "thinking system");
        assert_eq!(msgs[1]["content"], "hello");
    }

    #[test]
    fn test_build_thinking_payload_inserts_system_prompt() {
        let caller = make_caller();
        let messages = vec![serde_json::json!({"role": "user", "content": "hello"})];
        let payload = caller.build_thinking_payload(&messages, Some("thinking system"), None);
        let msgs = payload["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "thinking system");
    }

    #[test]
    fn test_build_thinking_payload_appends_analysis_prompt() {
        let caller = make_caller();
        let messages = vec![serde_json::json!({"role": "user", "content": "hello"})];
        let payload = caller.build_thinking_payload(&messages, None, Some("analyze this"));
        let msgs = payload["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "analyze this");
    }

    #[test]
    fn test_build_refinement_payload() {
        let caller = make_caller();
        let payload =
            caller.build_refinement_payload("sys prompt", "original trace", "critique text");
        let msgs = payload["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "sys prompt");
        let user_content = msgs[1]["content"].as_str().unwrap();
        assert!(user_content.contains("original trace"));
        assert!(user_content.contains("critique text"));
    }

    #[test]
    fn test_build_critique_payload() {
        let caller = make_caller();
        let payload = caller.build_critique_payload("trace here", "You are a critic.");
        assert_eq!(payload["model"], "gpt-4o");
        assert_eq!(payload["max_tokens"], 2048);
        let msgs = payload["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert!(msgs[1]["content"].as_str().unwrap().contains("trace here"));
    }

    #[test]
    fn test_build_action_payload() {
        let caller = make_caller();
        let messages = vec![serde_json::json!({"role": "user", "content": "do something"})];
        let tools = vec![serde_json::json!({
            "type": "function",
            "function": {"name": "read_file", "parameters": {}}
        })];
        let payload = caller.build_action_payload(&messages, &tools);
        assert_eq!(payload["model"], "gpt-4o");
        assert_eq!(payload["tool_choice"], "auto");
        assert!(payload["tools"].as_array().unwrap().len() == 1);
        assert_eq!(payload["temperature"], 0.7);
    }

    #[test]
    fn test_parse_action_response_success() {
        let caller = make_caller();
        let body = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "Hello world", "tool_calls": null}}],
            "usage": {"total_tokens": 100}
        });
        let resp = caller.parse_action_response(&body);
        assert!(resp.success);
        assert_eq!(resp.content.as_deref(), Some("Hello world"));
        assert!(resp.usage.is_some());
    }

    #[test]
    fn test_parse_action_response_with_tool_calls() {
        let caller = make_caller();
        let body = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": null,
                "tool_calls": [{"id": "tc-1", "function": {"name": "read_file", "arguments": "{\"path\": \"test.rs\"}"}}]
            }}]
        });
        let resp = caller.parse_action_response(&body);
        assert!(resp.success);
        assert!(resp.content.is_none());
        assert!(resp.tool_calls.is_some());
        assert_eq!(resp.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_parse_action_response_no_choices() {
        let caller = make_caller();
        let body = serde_json::json!({"choices": []});
        let resp = caller.parse_action_response(&body);
        assert!(!resp.success);
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_parse_thinking_response() {
        let caller = make_caller();
        let body = serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "I think we should..."}}]});
        let resp = caller.parse_thinking_response(&body);
        assert!(resp.success);
        assert_eq!(resp.content.as_deref(), Some("I think we should..."));
        assert!(resp.tool_calls.is_none());
    }

    #[test]
    fn test_parse_critique_response() {
        let caller = make_caller();
        let body = serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "The reasoning has flaws..."}}]});
        let resp = caller.parse_critique_response(&body);
        assert!(resp.success);
        assert_eq!(resp.content.as_deref(), Some("The reasoning has flaws..."));
    }

    #[test]
    fn test_parse_response_cleans_provider_tokens() {
        let caller = make_caller();
        let body = serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "Hello<|im_end|> world"}}]});
        let resp = caller.parse_action_response(&body);
        assert!(resp.success);
        assert_eq!(resp.content.as_deref(), Some("Hello world"));
    }

    #[test]
    fn test_action_payload_reasoning_model() {
        let caller = LlmCaller::new(LlmCallConfig {
            model: "o3-mini".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(4096),
        });
        let messages = vec![serde_json::json!({"role": "user", "content": "test"})];
        let tools = vec![serde_json::json!({"type": "function", "function": {"name": "test"}})];
        let payload = caller.build_action_payload(&messages, &tools);
        assert_eq!(payload["max_completion_tokens"], 4096);
        assert!(payload.get("max_tokens").is_none());
        assert!(payload.get("temperature").is_none());
    }

    #[test]
    fn test_critique_payload_reasoning_model() {
        let caller = make_caller().with_critique_config(LlmCallConfig {
            model: "o4-mini".to_string(),
            temperature: Some(0.5),
            max_tokens: Some(4096),
        });
        let payload = caller.build_critique_payload("trace", "system");
        assert_eq!(payload["max_completion_tokens"], 2048);
        assert!(payload.get("max_tokens").is_none());
        assert!(payload.get("temperature").is_none());
    }

    #[test]
    fn test_refinement_payload_reasoning_model() {
        let caller = make_caller().with_thinking_config(LlmCallConfig {
            model: "o1-preview".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(8192),
        });
        let payload = caller.build_refinement_payload("sys", "trace", "critique");
        assert_eq!(payload["max_completion_tokens"], 4096);
        assert!(payload.get("max_tokens").is_none());
        assert!(payload.get("temperature").is_none());
    }

    #[test]
    fn test_parse_response_with_reasoning_content() {
        let caller = make_caller();
        let body = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "The answer is 42.", "reasoning_content": "Let me think step by step..."}}]
        });
        let resp = caller.parse_action_response(&body);
        assert!(resp.success);
        assert_eq!(
            resp.reasoning_content.as_deref(),
            Some("Let me think step by step...")
        );
    }
}
