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
        // Strip internal reasoning effort field
        payload
            .as_object_mut()
            .map(|obj| obj.remove("_reasoning_effort"));
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
#[path = "groq_tests.rs"]
mod tests;
