//! Provider-specific request/response adapters.
//!
//! Each LLM provider has slightly different API conventions. Adapters
//! normalize requests to the provider's format and responses back to
//! a common Chat Completions format.

pub mod anthropic;
pub mod azure;
pub mod base;
pub mod bedrock;
pub mod chat_completions;
pub mod gemini;
pub mod groq;
pub mod mistral;
pub mod ollama;
pub mod openai;
pub mod schema_adapter;

pub use base::ProviderAdapter;
pub use schema_adapter::adapt_for_provider;

/// Detect the LLM provider from an API key prefix.
///
/// Returns `Some(provider_name)` if the key matches a known pattern:
/// - `sk-ant-` -> `"anthropic"`
/// - `sk-` -> `"openai"`
/// - `gsk_` -> `"groq"`
/// - `AIza` -> `"gemini"`
///
/// Returns `None` if the key format is not recognized.
pub fn detect_provider_from_key(api_key: &str) -> Option<&'static str> {
    // Order matters: check more specific prefixes first (sk-ant- before sk-).
    if api_key.starts_with("sk-ant-") {
        Some("anthropic")
    } else if api_key.starts_with("sk-") {
        Some("openai")
    } else if api_key.starts_with("gsk_") {
        Some("groq")
    } else if api_key.starts_with("AIza") {
        Some("gemini")
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
