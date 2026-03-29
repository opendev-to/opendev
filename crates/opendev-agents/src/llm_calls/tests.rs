use super::*;

fn make_caller() -> LlmCaller {
    LlmCaller::new(LlmCallConfig {
        model: "gpt-4o".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        reasoning_effort: None,
    })
}

#[test]
fn test_clean_messages_strips_underscore_keys() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "hello", "_internal": true}),
        serde_json::json!({"role": "assistant", "content": "world"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert!(cleaned[0].get("_internal").is_none());
    assert_eq!(cleaned[0]["role"], "user");
    assert_eq!(cleaned[1]["role"], "assistant");
}

#[test]
fn test_clean_messages_preserves_non_object() {
    let messages = vec![serde_json::json!("string_value")];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert_eq!(cleaned[0], "string_value");
}

#[test]
fn test_clean_messages_strips_internal() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "hello"}),
        serde_json::json!({"role": "user", "content": "[SYSTEM] debug", "_msg_class": "internal"}),
        serde_json::json!({"role": "user", "content": "[SYSTEM] error", "_msg_class": "directive"}),
        serde_json::json!({"role": "user", "content": "[SYSTEM] nudge", "_msg_class": "nudge"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert_eq!(cleaned.len(), 3);
    assert_eq!(cleaned[0]["content"], "hello");
    assert_eq!(cleaned[1]["content"], "[SYSTEM] error");
    assert_eq!(cleaned[2]["content"], "[SYSTEM] nudge");
    assert!(cleaned[1].get("_msg_class").is_none());
    assert!(cleaned[2].get("_msg_class").is_none());
}

#[test]
fn test_build_action_payload() {
    let caller = make_caller();
    let messages = vec![serde_json::json!({"role": "user", "content": "do something"})];
    let tools = vec![serde_json::json!({
        "type": "function",
        "function": {"name": "read_file", "parameters": {}}
    })];
    let payload = caller.build_action_payload(&messages, &tools);
    assert_eq!(payload["model"], "gpt-4o");
    assert_eq!(payload["tool_choice"], "auto");
    assert!(payload["tools"].as_array().unwrap().len() == 1);
    assert_eq!(payload["temperature"], 0.7);
}

#[test]
fn test_parse_action_response_success() {
    let caller = make_caller();
    let body = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "Hello world", "tool_calls": null}}],
        "usage": {"total_tokens": 100}
    });
    let resp = caller.parse_action_response(&body);
    assert!(resp.success);
    assert_eq!(resp.content.as_deref(), Some("Hello world"));
    assert!(resp.usage.is_some());
}

#[test]
fn test_parse_action_response_with_tool_calls() {
    let caller = make_caller();
    let body = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": null,
            "tool_calls": [{"id": "tc-1", "function": {"name": "read_file", "arguments": "{\"path\": \"test.rs\"}"}}]
        }}]
    });
    let resp = caller.parse_action_response(&body);
    assert!(resp.success);
    assert!(resp.content.is_none());
    assert!(resp.tool_calls.is_some());
    assert_eq!(resp.tool_calls.as_ref().unwrap().len(), 1);
}

#[test]
fn test_parse_action_response_no_choices() {
    let caller = make_caller();
    let body = serde_json::json!({"choices": []});
    let resp = caller.parse_action_response(&body);
    assert!(!resp.success);
    assert!(resp.error.is_some());
}

#[test]
fn test_parse_response_cleans_provider_tokens() {
    let caller = make_caller();
    let body = serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "Hello<|im_end|> world"}}]});
    let resp = caller.parse_action_response(&body);
    assert!(resp.success);
    assert_eq!(resp.content.as_deref(), Some("Hello world"));
}

#[test]
fn test_action_payload_reasoning_model() {
    let caller = LlmCaller::new(LlmCallConfig {
        model: "o3-mini".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        reasoning_effort: None,
    });
    let messages = vec![serde_json::json!({"role": "user", "content": "test"})];
    let tools = vec![serde_json::json!({"type": "function", "function": {"name": "test"}})];
    let payload = caller.build_action_payload(&messages, &tools);
    assert_eq!(payload["max_completion_tokens"], 4096);
    assert!(payload.get("max_tokens").is_none());
    assert!(payload.get("temperature").is_none());
}

#[test]
fn test_parse_response_with_reasoning_content() {
    let caller = make_caller();
    let body = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "The answer is 42.", "reasoning_content": "Let me think step by step..."}}]
    });
    let resp = caller.parse_action_response(&body);
    assert!(resp.success);
    assert_eq!(
        resp.reasoning_content.as_deref(),
        Some("Let me think step by step...")
    );
}

#[test]
fn test_parse_action_response_extracts_finish_reason() {
    let caller = make_caller();
    let body = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "partial..."}, "finish_reason": "length"}]
    });
    let resp = caller.parse_action_response(&body);
    assert!(resp.success);
    assert_eq!(resp.finish_reason.as_deref(), Some("length"));
}

#[test]
fn test_parse_action_response_finish_reason_stop() {
    let caller = make_caller();
    let body = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "done"}, "finish_reason": "stop"}]
    });
    let resp = caller.parse_action_response(&body);
    assert_eq!(resp.finish_reason.as_deref(), Some("stop"));
}

#[test]
fn test_parse_action_response_finish_reason_null() {
    let caller = make_caller();
    let body = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "done"}, "finish_reason": null}]
    });
    let resp = caller.parse_action_response(&body);
    assert!(resp.finish_reason.is_none());
}
