//! Groq adapter.
//!
//! Groq's API is OpenAI-compatible (Chat Completions format) with additional
//! rate-limiting headers (`x-ratelimit-*`). This adapter:
//! - Passes requests mostly unchanged
//! - Extracts rate limit information from responses
//! - Endpoint: `https://api.groq.com/openai/v1/chat/completions`

use serde_json::{Value, json};

const DEFAULT_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";

/// Rate limit information extracted from Groq response headers.
#[derive(Debug, Clone, Default)]
pub struct RateLimitInfo {
    /// Maximum requests allowed per window.
    pub limit_requests: Option<u64>,
    /// Maximum tokens allowed per window.
    pub limit_tokens: Option<u64>,
    /// Remaining requests in the current window.
    pub remaining_requests: Option<u64>,
    /// Remaining tokens in the current window.
    pub remaining_tokens: Option<u64>,
    /// Time until the request limit resets (e.g., "1s", "6m0s").
    pub reset_requests: Option<String>,
    /// Time until the token limit resets.
    pub reset_tokens: Option<String>,
}

impl RateLimitInfo {
    /// Parse rate limit info from HTTP response headers.
    ///
    /// Groq returns these headers:
    /// - `x-ratelimit-limit-requests`
    /// - `x-ratelimit-limit-tokens`
    /// - `x-ratelimit-remaining-requests`
    /// - `x-ratelimit-remaining-tokens`
    /// - `x-ratelimit-reset-requests`
    /// - `x-ratelimit-reset-tokens`
    pub fn from_headers(headers: &[(String, String)]) -> Self {
        let mut info = Self::default();
        for (key, value) in headers {
            match key.as_str() {
                "x-ratelimit-limit-requests" => {
                    info.limit_requests = value.parse().ok();
                }
                "x-ratelimit-limit-tokens" => {
                    info.limit_tokens = value.parse().ok();
                }
                "x-ratelimit-remaining-requests" => {
                    info.remaining_requests = value.parse().ok();
                }
                "x-ratelimit-remaining-tokens" => {
                    info.remaining_tokens = value.parse().ok();
                }
                "x-ratelimit-reset-requests" => {
                    info.reset_requests = Some(value.clone());
                }
                "x-ratelimit-reset-tokens" => {
                    info.reset_tokens = Some(value.clone());
                }
                _ => {}
            }
        }
        info
    }

    /// Convert rate limit info to a JSON value for logging/debugging.
    pub fn to_json(&self) -> Value {
        let mut obj = json!({});
        if let Some(v) = self.limit_requests {
            obj["limit_requests"] = json!(v);
        }
        if let Some(v) = self.limit_tokens {
            obj["limit_tokens"] = json!(v);
        }
        if let Some(v) = self.remaining_requests {
            obj["remaining_requests"] = json!(v);
        }
        if let Some(v) = self.remaining_tokens {
            obj["remaining_tokens"] = json!(v);
        }
        if let Some(ref v) = self.reset_requests {
            obj["reset_requests"] = json!(v);
        }
        if let Some(ref v) = self.reset_tokens {
            obj["reset_tokens"] = json!(v);
        }
        obj
    }
}

/// Adapter for the Groq Chat Completions API.
///
/// Groq is OpenAI-compatible so requests pass through with minimal changes.
/// The main value-add is rate limit header extraction.
#[derive(Debug, Clone)]
pub struct GroqAdapter {
    api_url: String,
}

impl GroqAdapter {
    /// Create a new Groq adapter with the default API URL.
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

    /// Remove unsupported parameters from the request payload.
    ///
    /// Groq does not support some OpenAI-specific parameters.
    fn clean_request(payload: &mut Value) {
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("logprobs");
            obj.remove("top_logprobs");
            obj.remove("n");
        }
    }
}

impl Default for GroqAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for GroqAdapter {
    fn provider_name(&self) -> &str {
        "groq"
    }

    fn convert_request(&self, mut payload: Value) -> Value {
        Self::clean_request(&mut payload);
        payload
    }

    fn convert_response(&self, response: Value) -> Value {
        // Groq responses are already in Chat Completions format
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
        let adapter = GroqAdapter::new();
        assert_eq!(adapter.provider_name(), "groq");
    }

    #[test]
    fn test_api_url_default() {
        let adapter = GroqAdapter::new();
        assert_eq!(adapter.api_url(), DEFAULT_API_URL);
    }

    #[test]
    fn test_api_url_custom() {
        let adapter = GroqAdapter::with_url("https://my-proxy.com/v1/chat/completions");
        assert_eq!(
            adapter.api_url(),
            "https://my-proxy.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_convert_request_passthrough() {
        let adapter = GroqAdapter::new();
        let payload = json!({
            "model": "llama3-70b-8192",
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Hello"}
            ],
            "temperature": 0.7,
            "max_tokens": 1024
        });
        let result = adapter.convert_request(payload);

        assert_eq!(result["model"], "llama3-70b-8192");
        assert_eq!(result["temperature"], 0.7);
        assert_eq!(result["max_tokens"], 1024);
        assert_eq!(result["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_convert_request_removes_unsupported() {
        let adapter = GroqAdapter::new();
        let payload = json!({
            "model": "llama3-70b-8192",
            "messages": [{"role": "user", "content": "Hi"}],
            "logprobs": true,
            "top_logprobs": 5,
            "n": 2
        });
        let result = adapter.convert_request(payload);

        assert!(result.get("logprobs").is_none());
        assert!(result.get("top_logprobs").is_none());
        assert!(result.get("n").is_none());
        assert_eq!(result["model"], "llama3-70b-8192");
    }

    #[test]
    fn test_convert_response_passthrough() {
        let adapter = GroqAdapter::new();
        let response = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "llama3-70b-8192",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 8,
                "total_tokens": 18
            }
        });
        let result = adapter.convert_response(response.clone());
        assert_eq!(result, response);
    }

    #[test]
    fn test_rate_limit_info_from_headers() {
        let headers = vec![
            ("x-ratelimit-limit-requests".to_string(), "30".to_string()),
            (
                "x-ratelimit-limit-tokens".to_string(),
                "30000".to_string(),
            ),
            (
                "x-ratelimit-remaining-requests".to_string(),
                "29".to_string(),
            ),
            (
                "x-ratelimit-remaining-tokens".to_string(),
                "29500".to_string(),
            ),
            (
                "x-ratelimit-reset-requests".to_string(),
                "2s".to_string(),
            ),
            (
                "x-ratelimit-reset-tokens".to_string(),
                "1s".to_string(),
            ),
        ];
        let info = RateLimitInfo::from_headers(&headers);

        assert_eq!(info.limit_requests, Some(30));
        assert_eq!(info.limit_tokens, Some(30000));
        assert_eq!(info.remaining_requests, Some(29));
        assert_eq!(info.remaining_tokens, Some(29500));
        assert_eq!(info.reset_requests, Some("2s".to_string()));
        assert_eq!(info.reset_tokens, Some("1s".to_string()));
    }

    #[test]
    fn test_rate_limit_info_partial_headers() {
        let headers = vec![(
            "x-ratelimit-remaining-requests".to_string(),
            "5".to_string(),
        )];
        let info = RateLimitInfo::from_headers(&headers);

        assert_eq!(info.limit_requests, None);
        assert_eq!(info.remaining_requests, Some(5));
        assert_eq!(info.remaining_tokens, None);
    }

    #[test]
    fn test_rate_limit_info_to_json() {
        let info = RateLimitInfo {
            limit_requests: Some(30),
            limit_tokens: Some(30000),
            remaining_requests: Some(29),
            remaining_tokens: None,
            reset_requests: Some("2s".to_string()),
            reset_tokens: None,
        };
        let j = info.to_json();
        assert_eq!(j["limit_requests"], 30);
        assert_eq!(j["limit_tokens"], 30000);
        assert_eq!(j["remaining_requests"], 29);
        assert!(j.get("remaining_tokens").is_none());
        assert_eq!(j["reset_requests"], "2s");
        assert!(j.get("reset_tokens").is_none());
    }

    #[test]
    fn test_rate_limit_info_empty_headers() {
        let headers: Vec<(String, String)> = vec![];
        let info = RateLimitInfo::from_headers(&headers);

        assert_eq!(info.limit_requests, None);
        assert_eq!(info.limit_tokens, None);
        assert_eq!(info.remaining_requests, None);
        assert_eq!(info.remaining_tokens, None);
        assert_eq!(info.reset_requests, None);
        assert_eq!(info.reset_tokens, None);
    }

    #[test]
    fn test_extra_headers_empty() {
        let adapter = GroqAdapter::new();
        assert!(adapter.extra_headers().is_empty());
    }

    #[test]
    fn test_convert_response_with_tool_calls() {
        let adapter = GroqAdapter::new();
        let response = json!({
            "id": "chatcmpl-456",
            "object": "chat.completion",
            "model": "llama3-70b-8192",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
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
        let result = adapter.convert_response(response.clone());
        // Should pass through unchanged
        assert_eq!(result, response);
    }
}
