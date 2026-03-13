//! Adapted HTTP client that wraps HttpClient + ProviderAdapter.
//!
//! Transparently converts requests/responses through the provider adapter
//! so the rest of the codebase can use a uniform Chat Completions format.

use crate::adapters::base::ProviderAdapter;
use crate::adapters::detect_provider_from_key;
use crate::client::HttpClient;
use crate::models::{HttpError, HttpResult};
use tokio_util::sync::CancellationToken;

/// HTTP client with provider-specific request/response adaptation.
///
/// Wraps `HttpClient` and an optional `ProviderAdapter`. When an adapter
/// is present, `post_json` will:
/// 1. Convert the payload via `adapter.convert_request()`
/// 2. Send via `HttpClient::post_json()`
/// 3. Convert the response body via `adapter.convert_response()`
pub struct AdaptedClient {
    client: HttpClient,
    adapter: Option<Box<dyn ProviderAdapter>>,
}

impl AdaptedClient {
    /// Create an adapted client without any adapter (passthrough).
    pub fn new(client: HttpClient) -> Self {
        Self {
            client,
            adapter: None,
        }
    }

    /// Create an adapted client with a provider adapter.
    pub fn with_adapter(client: HttpClient, adapter: Box<dyn ProviderAdapter>) -> Self {
        Self {
            client,
            adapter: Some(adapter),
        }
    }

    /// Create an adapter for a specific provider name.
    ///
    /// Recognized providers:
    /// - `"anthropic"` → [`AnthropicAdapter`](crate::adapters::anthropic::AnthropicAdapter)
    /// - `"openai"` → [`OpenAiAdapter`](crate::adapters::openai::OpenAiAdapter)
    /// - `"gemini"` | `"google"` → [`GeminiAdapter`](crate::adapters::gemini::GeminiAdapter)
    ///
    /// Returns `None` for providers that use the Chat Completions format natively
    /// (groq, fireworks, mistral, etc.).
    pub fn adapter_for_provider(provider: &str) -> Option<Box<dyn ProviderAdapter>> {
        match provider {
            "anthropic" => Some(Box::new(
                crate::adapters::anthropic::AnthropicAdapter::new(),
            )),
            "openai" => Some(Box::new(crate::adapters::openai::OpenAiAdapter::new())),
            "gemini" | "google" => Some(Box::new(
                crate::adapters::gemini::GeminiAdapter::default(),
            )),
            _ => None,
        }
    }

    /// Resolve the provider name, falling back to auto-detection from the API key.
    ///
    /// If `provider` is non-empty, returns it as-is. Otherwise, inspects the
    /// API key prefix via [`detect_provider_from_key`] and returns the detected
    /// provider or `"openai"` as the final fallback.
    pub fn resolve_provider(provider: &str, api_key: &str) -> String {
        if !provider.is_empty() {
            return provider.to_string();
        }
        detect_provider_from_key(api_key)
            .unwrap_or("openai")
            .to_string()
    }

    /// POST JSON with optional request/response conversion.
    pub async fn post_json(
        &self,
        payload: &serde_json::Value,
        cancel: Option<&CancellationToken>,
    ) -> Result<HttpResult, HttpError> {
        // Only clone the payload when an adapter needs to transform it.
        // For the passthrough (None) case, use the original reference directly.
        let converted;
        let effective_payload = match &self.adapter {
            Some(adapter) => {
                converted = adapter.convert_request(payload.clone());
                &converted
            }
            None => payload,
        };

        let mut result = self.client.post_json(effective_payload, cancel).await?;

        // Convert response body back to Chat Completions format
        if let (Some(adapter), Some(body)) = (&self.adapter, &result.body)
            && result.success
        {
            result.body = Some(adapter.convert_response(body.clone()));
        }

        Ok(result)
    }

    /// Get the configured API URL.
    pub fn api_url(&self) -> &str {
        self.client.api_url()
    }
}

impl std::fmt::Debug for AdaptedClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdaptedClient")
            .field("api_url", &self.client.api_url())
            .field(
                "adapter",
                &self
                    .adapter
                    .as_ref()
                    .map(|a| a.provider_name())
                    .unwrap_or("none"),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_for_provider_anthropic() {
        let adapter = AdaptedClient::adapter_for_provider("anthropic").unwrap();
        assert_eq!(adapter.provider_name(), "anthropic");
    }

    #[test]
    fn test_adapter_for_provider_openai() {
        let adapter = AdaptedClient::adapter_for_provider("openai").unwrap();
        assert_eq!(adapter.provider_name(), "openai");
    }

    #[test]
    fn test_adapter_for_provider_gemini() {
        let adapter = AdaptedClient::adapter_for_provider("gemini").unwrap();
        assert_eq!(adapter.provider_name(), "gemini");
    }

    #[test]
    fn test_adapter_for_provider_google() {
        let adapter = AdaptedClient::adapter_for_provider("google").unwrap();
        assert_eq!(adapter.provider_name(), "gemini");
    }

    #[test]
    fn test_adapter_for_provider_groq_is_none() {
        assert!(AdaptedClient::adapter_for_provider("groq").is_none());
    }

    #[test]
    fn test_adapter_for_provider_unknown_is_none() {
        assert!(AdaptedClient::adapter_for_provider("custom").is_none());
    }

    #[test]
    fn test_resolve_provider_explicit() {
        assert_eq!(AdaptedClient::resolve_provider("anthropic", ""), "anthropic");
        assert_eq!(
            AdaptedClient::resolve_provider("custom", "sk-ant-abc"),
            "custom"
        );
    }

    #[test]
    fn test_resolve_provider_auto_detect() {
        assert_eq!(
            AdaptedClient::resolve_provider("", "sk-ant-api03-abc"),
            "anthropic"
        );
        assert_eq!(
            AdaptedClient::resolve_provider("", "sk-proj-abc"),
            "openai"
        );
        assert_eq!(
            AdaptedClient::resolve_provider("", "AIzaSyAbc123"),
            "gemini"
        );
        assert_eq!(
            AdaptedClient::resolve_provider("", "gsk_abc123"),
            "groq"
        );
    }

    #[test]
    fn test_resolve_provider_fallback_to_openai() {
        assert_eq!(
            AdaptedClient::resolve_provider("", "unknown-key"),
            "openai"
        );
    }
}
