//! Model detection and classification helpers.
//!
//! Functions for identifying reasoning models, determining parameter support
//! (e.g. `max_completion_tokens` vs `max_tokens`), and conditionally inserting
//! model-appropriate parameters into LLM payloads.

use serde_json::Value;

/// Model prefixes that use `max_completion_tokens` instead of `max_tokens`.
const MAX_COMPLETION_TOKENS_PREFIXES: &[&str] = &["o1", "o3", "o4", "gpt-5"];

/// Model prefixes that do not support the `temperature` parameter.
const NO_TEMPERATURE_PREFIXES: &[&str] = &["o1", "o3", "o4", "codex", "gpt-5"];

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
#[path = "model_detection_tests.rs"]
mod tests;
