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
                            let args_str =
                                serde_json::to_string(args).unwrap_or_default();
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
mod tests {
    use super::*;
    use crate::adapters::base::ProviderAdapter;

    #[test]
    fn test_provider_name() {
        let adapter = MistralAdapter::new();
        assert_eq!(adapter.provider_name(), "mistral");
    }

    #[test]
    fn test_api_url_default() {
        let adapter = MistralAdapter::new();
        assert_eq!(adapter.api_url(), DEFAULT_API_URL);
    }

    #[test]
    fn test_api_url_custom() {
        let adapter = MistralAdapter::with_url("https://my-proxy.com/v1/chat/completions");
        assert_eq!(
            adapter.api_url(),
            "https://my-proxy.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_convert_request_passthrough() {
        let adapter = MistralAdapter::new();
        let payload = json!({
            "model": "mistral-large-latest",
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Hello"}
            ],
            "temperature": 0.7,
            "max_tokens": 1024
        });
        let result = adapter.convert_request(payload.clone());

        // Core fields should be preserved
        assert_eq!(result["model"], "mistral-large-latest");
        assert_eq!(result["temperature"], 0.7);
        assert_eq!(result["max_tokens"], 1024);
        assert_eq!(result["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_convert_request_removes_unsupported() {
        let adapter = MistralAdapter::new();
        let payload = json!({
            "model": "mistral-large-latest",
            "messages": [{"role": "user", "content": "Hi"}],
            "logprobs": true,
            "top_logprobs": 5,
            "n": 2,
            "seed": 42
        });
        let result = adapter.convert_request(payload);

        assert!(result.get("logprobs").is_none());
        assert!(result.get("top_logprobs").is_none());
        assert!(result.get("n").is_none());
        assert!(result.get("seed").is_none());
        // Model and messages preserved
        assert_eq!(result["model"], "mistral-large-latest");
    }

    #[test]
    fn test_convert_response_passthrough() {
        let adapter = MistralAdapter::new();
        let response = json!({
            "id": "cmpl-123",
            "object": "chat.completion",
            "model": "mistral-large-latest",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });
        let result = adapter.convert_response(response);

        assert_eq!(result["choices"][0]["message"]["content"], "Hello!");
        assert_eq!(result["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn test_normalize_tool_calls_missing_type() {
        let adapter = MistralAdapter::new();
        let response = json!({
            "id": "cmpl-456",
            "object": "chat.completion",
            "model": "mistral-large-latest",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\": \"test.txt\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
        });
        let result = adapter.convert_response(response);

        let tc = &result["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tc["type"], "function");
        assert_eq!(tc["id"], "call_abc");
        assert_eq!(tc["function"]["name"], "read_file");
    }

    #[test]
    fn test_normalize_tool_calls_object_arguments() {
        let adapter = MistralAdapter::new();
        let response = json!({
            "id": "cmpl-789",
            "object": "chat.completion",
            "model": "mistral-large-latest",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_xyz",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": {"path": "test.txt"}
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
        });
        let result = adapter.convert_response(response);

        let args = result["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap();
        // Should be serialized to a JSON string
        let parsed: Value = serde_json::from_str(args).unwrap();
        assert_eq!(parsed["path"], "test.txt");
    }

    #[test]
    fn test_extra_headers_empty() {
        let adapter = MistralAdapter::new();
        assert!(adapter.extra_headers().is_empty());
    }
}
