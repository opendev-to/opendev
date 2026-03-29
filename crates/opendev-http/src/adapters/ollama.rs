//! Ollama adapter.
//!
//! Ollama exposes an OpenAI-compatible Chat Completions API at
//! `http://localhost:11434/v1/chat/completions`. This adapter:
//! - Uses a local default base URL (no auth required)
//! - Passes requests in standard Chat Completions format
//! - Removes unsupported parameters
//! - Handles minor response differences

use serde_json::Value;

const DEFAULT_API_URL: &str = "http://localhost:11434/v1/chat/completions";

/// Adapter for the Ollama local inference server.
///
/// Ollama is OpenAI-compatible, so requests pass through with minimal changes.
/// No authentication is required for local instances.
#[derive(Debug, Clone)]
pub struct OllamaAdapter {
    api_url: String,
}

impl OllamaAdapter {
    /// Create a new Ollama adapter with the default local URL.
    pub fn new() -> Self {
        Self {
            api_url: DEFAULT_API_URL.to_string(),
        }
    }

    /// Create with a custom API URL (e.g., remote Ollama instance).
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            api_url: url.into(),
        }
    }

    /// Remove parameters that Ollama does not support.
    fn clean_request(payload: &mut Value) {
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("logprobs");
            obj.remove("top_logprobs");
            obj.remove("n");
            obj.remove("frequency_penalty");
            obj.remove("presence_penalty");
            obj.remove("seed");
            // Ollama doesn't support max_completion_tokens; convert to max_tokens
            if let Some(val) = obj.remove("max_completion_tokens") {
                obj.entry("max_tokens").or_insert(val);
            }
        }
    }
}

impl Default for OllamaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for OllamaAdapter {
    fn provider_name(&self) -> &str {
        "ollama"
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
        // Ollama responses are in Chat Completions format
        response
    }

    fn api_url(&self) -> &str {
        &self.api_url
    }
}

#[cfg(test)]
#[path = "ollama_tests.rs"]
mod tests;
