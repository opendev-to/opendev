//! Condition helpers for prompt section filtering.
//!
//! These factory functions create [`ConditionFn`] closures that evaluate
//! runtime [`PromptContext`] values to decide whether a prompt section
//! should be included.

use super::{ConditionFn, PromptContext};

/// Create a condition that checks for a boolean context value.
pub fn ctx_bool(key: &str) -> ConditionFn {
    let key = key.to_string();
    Box::new(move |ctx: &PromptContext| ctx.get(&key).and_then(|v| v.as_bool()).unwrap_or(false))
}

/// Create a condition that checks for a string context value equality.
pub fn ctx_eq(key: &str, expected: &str) -> ConditionFn {
    let key = key.to_string();
    let expected = expected.to_string();
    Box::new(move |ctx: &PromptContext| {
        ctx.get(&key)
            .and_then(|v| v.as_str())
            .is_some_and(|v| v == expected)
    })
}

/// Create a condition that checks if a string value is in a set.
pub fn ctx_in(key: &str, values: &[&str]) -> ConditionFn {
    let key = key.to_string();
    let values: Vec<String> = values.iter().map(|s| s.to_string()).collect();
    Box::new(move |ctx: &PromptContext| {
        ctx.get(&key)
            .and_then(|v| v.as_str())
            .is_some_and(|v| values.iter().any(|exp| exp == v))
    })
}

/// Create a condition that checks for a non-null context value.
pub fn ctx_present(key: &str) -> ConditionFn {
    let key = key.to_string();
    Box::new(move |ctx: &PromptContext| ctx.get(&key).is_some_and(|v| !v.is_null()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_ctx_eq_condition() {
        let cond = ctx_eq("provider", "openai");

        let mut ctx = HashMap::new();
        assert!(!cond(&ctx));

        ctx.insert("provider".to_string(), serde_json::json!("anthropic"));
        assert!(!cond(&ctx));

        ctx.insert("provider".to_string(), serde_json::json!("openai"));
        assert!(cond(&ctx));
    }

    #[test]
    fn test_ctx_in_condition() {
        let cond = ctx_in("provider", &["fireworks", "fireworks-ai"]);

        let mut ctx = HashMap::new();
        ctx.insert("provider".to_string(), serde_json::json!("fireworks"));
        assert!(cond(&ctx));

        ctx.insert("provider".to_string(), serde_json::json!("fireworks-ai"));
        assert!(cond(&ctx));

        ctx.insert("provider".to_string(), serde_json::json!("openai"));
        assert!(!cond(&ctx));
    }

    #[test]
    fn test_ctx_present_condition() {
        let cond = ctx_present("session_id");

        let mut ctx = HashMap::new();
        assert!(!cond(&ctx));

        ctx.insert("session_id".to_string(), serde_json::json!(null));
        assert!(!cond(&ctx));

        ctx.insert("session_id".to_string(), serde_json::json!("abc-123"));
        assert!(cond(&ctx));
    }
}
