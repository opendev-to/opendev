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
    // After filtering the internal message, the 3 remaining user messages merge into 1.
    let messages = vec![
        serde_json::json!({"role": "user", "content": "hello"}),
        serde_json::json!({"role": "user", "content": "<system-reminder>\ndebug\n</system-reminder>", "_msg_class": "internal"}),
        serde_json::json!({"role": "user", "content": "<system-reminder>\nerror\n</system-reminder>", "_msg_class": "directive"}),
        serde_json::json!({"role": "user", "content": "<system-reminder>\nnudge\n</system-reminder>", "_msg_class": "nudge"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    // 3 consecutive user messages are merged into 1
    assert_eq!(cleaned.len(), 1);
    let content = cleaned[0]["content"].as_str().unwrap();
    assert!(content.contains("hello"));
    assert!(content.contains("error"));
    assert!(content.contains("nudge"));
    assert!(!content.contains("debug")); // internal was filtered
    assert!(cleaned[0].get("_msg_class").is_none());
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

// ---- Message normalization tests ----

#[test]
fn test_merge_consecutive_user_messages() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "first"}),
        serde_json::json!({"role": "user", "content": "second"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert_eq!(cleaned.len(), 1);
    assert_eq!(cleaned[0]["content"], "first\n\nsecond");
}

#[test]
fn test_merge_consecutive_assistant_messages() {
    let messages = vec![
        serde_json::json!({
            "role": "assistant",
            "content": "thinking...",
            "tool_calls": [{"id": "tc-1", "function": {"name": "read_file", "arguments": "{}"}}]
        }),
        serde_json::json!({
            "role": "assistant",
            "content": "more thinking",
            "tool_calls": [{"id": "tc-2", "function": {"name": "write_file", "arguments": "{}"}}]
        }),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert_eq!(cleaned.len(), 1);
    assert_eq!(cleaned[0]["content"], "thinking...\n\nmore thinking");
    let tc = cleaned[0]["tool_calls"].as_array().unwrap();
    assert_eq!(tc.len(), 2);
    assert_eq!(tc[0]["id"], "tc-1");
    assert_eq!(tc[1]["id"], "tc-2");
}

#[test]
fn test_no_merge_across_roles() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "question"}),
        serde_json::json!({"role": "assistant", "content": "answer"}),
        serde_json::json!({"role": "user", "content": "follow-up"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert_eq!(cleaned.len(), 3);
}

#[test]
fn test_no_merge_tool_messages() {
    let messages = vec![
        serde_json::json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {"id": "tc-1", "function": {"name": "a", "arguments": "{}"}},
                {"id": "tc-2", "function": {"name": "b", "arguments": "{}"}}
            ]
        }),
        serde_json::json!({"role": "tool", "tool_call_id": "tc-1", "content": "result1"}),
        serde_json::json!({"role": "tool", "tool_call_id": "tc-2", "content": "result2"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert_eq!(cleaned.len(), 3);
    assert_eq!(cleaned[1]["role"], "tool");
    assert_eq!(cleaned[2]["role"], "tool");
}

#[test]
fn test_filter_whitespace_only_messages() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "real message"}),
        serde_json::json!({"role": "user", "content": "   "}),
        serde_json::json!({"role": "assistant", "content": ""}),
        serde_json::json!({"role": "user", "content": "another real one"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    // whitespace-only user and empty assistant are removed; two remaining users merge
    assert_eq!(cleaned.len(), 1);
    assert_eq!(cleaned[0]["content"], "real message\n\nanother real one");
}

#[test]
fn test_keep_assistant_with_tool_calls_only() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "do something"}),
        serde_json::json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{"id": "tc-1", "function": {"name": "bash", "arguments": "{}"}}]
        }),
        serde_json::json!({"role": "tool", "tool_call_id": "tc-1", "content": "done"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert_eq!(cleaned.len(), 3);
    assert_eq!(cleaned[1]["role"], "assistant");
}

#[test]
fn test_remove_orphaned_tool_results() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "hello"}),
        // Orphaned tool result — no assistant message has tool_call tc-999
        serde_json::json!({"role": "tool", "tool_call_id": "tc-999", "content": "orphan"}),
        serde_json::json!({"role": "assistant", "content": "response"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert_eq!(cleaned.len(), 2);
    assert_eq!(cleaned[0]["role"], "user");
    assert_eq!(cleaned[1]["role"], "assistant");
}

#[test]
fn test_internal_removal_exposes_merge() {
    // An internal message sits between two user messages. After removal, they merge.
    let messages = vec![
        serde_json::json!({"role": "user", "content": "part one"}),
        serde_json::json!({"role": "user", "content": "internal stuff", "_msg_class": "internal"}),
        serde_json::json!({"role": "user", "content": "part two"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert_eq!(cleaned.len(), 1);
    assert_eq!(cleaned[0]["content"], "part one\n\npart two");
}

#[test]
fn test_clean_messages_preserves_thinking_blocks() {
    // _thinking_blocks carries Anthropic's encrypted thinking signatures for
    // multi-turn echo-back and must survive clean_messages.
    let blocks = serde_json::json!([{"type": "thinking", "thinking": "...", "signature": "sig123"}]);
    let messages = vec![
        serde_json::json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{"id": "tc-1", "function": {"name": "read_file", "arguments": "{}"}}],
            "reasoning_content": "some reasoning",
            "_thinking_blocks": blocks.clone(),
            "_other_internal": "should be stripped",
        }),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert_eq!(cleaned.len(), 1);
    let msg = &cleaned[0];
    assert_eq!(msg.get("_thinking_blocks").unwrap(), &blocks);
    assert!(msg.get("_other_internal").is_none());
    assert!(msg.get("tool_calls").is_some());
    assert_eq!(msg.get("reasoning_content").unwrap(), "some reasoning");
}
