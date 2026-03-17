//! Model detection and classification helpers.
//!
//! Functions for identifying reasoning models, determining parameter support
//! (e.g. `max_completion_tokens` vs `max_tokens`), and conditionally inserting
//! model-appropriate parameters into LLM payloads.

use serde_json::Value;

/// Model prefixes that use `max_completion_tokens` instead of `max_tokens`.
const MAX_COMPLETION_TOKENS_PREFIXES: &[&str] = &["o1", "o3", "o4", "gpt-5"];

/// Model prefixes that do not support the `temperature` parameter.
const NO_TEMPERATURE_PREFIXES: &[&str] = &["o1", "o3", "o4", "codex"];

/// Check if a model is a reasoning model (o1, o3, o4, codex families).
pub fn is_reasoning_model(model: &str) -> bool {
    let lower = model.to_lowercase();
    NO_TEMPERATURE_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// Check if a model uses `max_completion_tokens` instead of `max_tokens`.
pub fn uses_max_completion_tokens(model: &str) -> bool {
    let lower = model.to_lowercase();
    MAX_COMPLETION_TOKENS_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// Check if a model supports the `temperature` parameter.
pub fn supports_temperature(model: &str) -> bool {
    !is_reasoning_model(model)
}

/// Insert the appropriate max tokens parameter for the given model.
pub(super) fn insert_max_tokens(payload: &mut Value, model: &str, max_tokens: u64) {
    if uses_max_completion_tokens(model) {
        payload["max_completion_tokens"] = serde_json::json!(max_tokens);
    } else {
        payload["max_tokens"] = serde_json::json!(max_tokens);
    }
}

/// Conditionally insert temperature if the model supports it.
pub(super) fn insert_temperature(payload: &mut Value, model: &str, temperature: f64) {
    if supports_temperature(model) {
        payload["temperature"] = serde_json::json!(temperature);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_reasoning_model() {
        assert!(is_reasoning_model("o1-preview"));
        assert!(is_reasoning_model("o1-mini"));
        assert!(is_reasoning_model("o3-mini"));
        assert!(is_reasoning_model("o4-mini"));
        assert!(is_reasoning_model("codex-mini"));
        assert!(!is_reasoning_model("gpt-4o"));
        assert!(!is_reasoning_model("gpt-5-turbo"));
        assert!(!is_reasoning_model("claude-3-opus"));
    }

    #[test]
    fn test_uses_max_completion_tokens() {
        assert!(uses_max_completion_tokens("o1-preview"));
        assert!(uses_max_completion_tokens("o3-mini"));
        assert!(uses_max_completion_tokens("o4-mini"));
        assert!(uses_max_completion_tokens("gpt-5-turbo"));
        assert!(!uses_max_completion_tokens("gpt-4o"));
        assert!(!uses_max_completion_tokens("claude-3-opus"));
        assert!(!uses_max_completion_tokens("codex-mini")); // codex uses max_completion_tokens? No — not in prefix list
    }

    #[test]
    fn test_supports_temperature() {
        assert!(supports_temperature("gpt-4o"));
        assert!(supports_temperature("gpt-5-turbo"));
        assert!(supports_temperature("claude-3-opus"));
        assert!(!supports_temperature("o1-preview"));
        assert!(!supports_temperature("o3-mini"));
        assert!(!supports_temperature("codex-mini"));
    }

    #[test]
    fn test_case_insensitive_model_detection() {
        assert!(is_reasoning_model("O1-Preview"));
        assert!(is_reasoning_model("O3-MINI"));
        assert!(uses_max_completion_tokens("GPT-5-turbo"));
    }
}
