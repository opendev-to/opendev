use super::*;
use chrono::Utc;
use std::collections::HashMap;

fn make_user_msg(content: &str) -> ChatMessage {
    ChatMessage {
        role: Role::User,
        content: content.to_string(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: vec![],
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    }
}

fn make_assistant_msg(content: &str) -> ChatMessage {
    ChatMessage {
        role: Role::Assistant,
        content: content.to_string(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: vec![],
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    }
}

#[test]
fn test_valid_user_message() {
    let msg = make_user_msg("Hello");
    let verdict = validate_message(&msg);
    assert!(verdict.is_valid);
}

#[test]
fn test_empty_user_message() {
    let msg = make_user_msg("  ");
    let verdict = validate_message(&msg);
    assert!(!verdict.is_valid);
    assert!(verdict.reason.contains("empty content"));
}

#[test]
fn test_empty_assistant_message() {
    let msg = make_assistant_msg("");
    let verdict = validate_message(&msg);
    assert!(!verdict.is_valid);
}

#[test]
fn test_assistant_with_empty_thinking_trace() {
    let mut msg = make_assistant_msg("Hello");
    msg.thinking_trace = Some("  ".to_string());
    let verdict = validate_message(&msg);
    assert!(!verdict.is_valid);
    assert!(verdict.reason.contains("empty thinking_trace"));
}

#[test]
fn test_tool_call_validation() {
    let tc = ToolCall {
        id: "tc-1".to_string(),
        name: "read_file".to_string(),
        parameters: HashMap::new(),
        result: None,
        result_summary: None,
        timestamp: Utc::now(),
        approved: false,
        error: None,
        nested_tool_calls: vec![],
    };
    let mut msg = make_assistant_msg("Calling tool");
    msg.tool_calls = vec![tc];
    let verdict = validate_message(&msg);
    assert!(!verdict.is_valid);
    assert!(verdict.reason.contains("no result and no error"));
}

#[test]
fn test_repair_empty_message_dropped() {
    let mut msg = make_assistant_msg("");
    assert!(!repair_message(&mut msg));
}

#[test]
fn test_repair_incomplete_tool_call() {
    let mut msg = make_assistant_msg("Calling tool");
    msg.tool_calls = vec![ToolCall {
        id: "tc-1".to_string(),
        name: "bash".to_string(),
        parameters: HashMap::new(),
        result: None,
        result_summary: None,
        timestamp: Utc::now(),
        approved: false,
        error: None,
        nested_tool_calls: vec![],
    }];

    assert!(repair_message(&mut msg));
    assert!(msg.tool_calls[0].error.is_some());
}

#[test]
fn test_filter_and_repair() {
    let mut messages = vec![
        make_user_msg("Hello"),
        make_assistant_msg(""), // will be dropped
        make_user_msg("World"),
    ];

    let (dropped, _repaired) = filter_and_repair_messages(&mut messages);
    assert_eq!(dropped, 1);
    assert_eq!(messages.len(), 2);
}
