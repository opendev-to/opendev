//! Provider-specific request/response adapters.
//!
//! Each LLM provider has slightly different API conventions. Adapters
//! normalize requests to the provider's format and responses back to
//! a common Chat Completions format.

pub mod anthropic;
pub mod azure;
pub mod base;
pub mod gemini;
pub mod groq;
pub mod mistral;
pub mod ollama;
pub mod openai;

pub use base::ProviderAdapter;

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
mod tests {
    use super::*;

    #[test]
    fn test_detect_anthropic() {
        assert_eq!(
            detect_provider_from_key("sk-ant-api03-abc123"),
            Some("anthropic")
        );
    }

    #[test]
    fn test_detect_openai() {
        assert_eq!(detect_provider_from_key("sk-proj-abc123"), Some("openai"));
        assert_eq!(detect_provider_from_key("sk-abc123"), Some("openai"));
    }

    #[test]
    fn test_detect_groq() {
        assert_eq!(detect_provider_from_key("gsk_abc123def456"), Some("groq"));
    }

    #[test]
    fn test_detect_gemini() {
        assert_eq!(detect_provider_from_key("AIzaSyAbc123"), Some("gemini"));
    }

    #[test]
    fn test_detect_unknown() {
        assert_eq!(detect_provider_from_key("unknown-key-format"), None);
        assert_eq!(detect_provider_from_key(""), None);
    }

    #[test]
    fn test_anthropic_before_openai() {
        // sk-ant- should match anthropic, not openai
        assert_eq!(
            detect_provider_from_key("sk-ant-api03-test"),
            Some("anthropic")
        );
    }
}
