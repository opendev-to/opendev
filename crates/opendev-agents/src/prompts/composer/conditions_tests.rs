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
