use super::*;
use serde_json::json;

fn make_msg(role: &str, content: &str) -> ApiMessage {
    let mut msg = ApiMessage::new();
    msg.insert("role".into(), json!(role));
    msg.insert("content".into(), json!(content));
    msg
}

fn make_tool_msg(name: &str, content: &str) -> ApiMessage {
    let mut msg = ApiMessage::new();
    msg.insert("role".into(), json!("tool"));
    msg.insert("name".into(), json!(name));
    msg.insert("content".into(), json!(content));
    msg
}

fn make_array_content_msg(role: &str, text: &str) -> ApiMessage {
    let mut msg = ApiMessage::new();
    msg.insert("role".into(), json!(role));
    msg.insert("content".into(), json!([{"type": "text", "text": text}]));
    msg
}

#[test]
fn test_fallback_summary_basic_structure() {
    let messages = vec![
        make_msg("user", "Fix the login bug in auth.rs"),
        make_tool_msg("read_file", "fn login() { /* broken */ }"),
        make_msg("assistant", "I found the issue in the login function"),
    ];
    let summary = ContextCompactor::fallback_summary(&messages);
    assert!(summary.contains("## Goal"));
    assert!(summary.contains("Fix the login bug"));
    assert!(summary.contains("## Key Actions"));
    assert!(summary.contains("read_file:"));
    assert!(summary.contains("## Current State"));
    assert!(summary.contains("I found the issue"));
}

#[test]
fn test_fallback_summary_with_array_content() {
    let messages = vec![
        make_array_content_msg("user", "Refactor the parser"),
        make_msg("assistant", "Working on it"),
    ];
    let summary = ContextCompactor::fallback_summary(&messages);
    assert!(summary.contains("Refactor the parser"));
}

#[test]
fn test_fallback_summary_tool_results_included() {
    let messages = vec![
        make_msg("user", "Read the config"),
        make_tool_msg("read_file", "key = value"),
        make_tool_msg("search", "found 3 matches"),
        make_msg("assistant", "Done analyzing"),
    ];
    let summary = ContextCompactor::fallback_summary(&messages);
    assert!(summary.contains("read_file: key = value"));
    assert!(summary.contains("search: found 3 matches"));
}

#[test]
fn test_fallback_summary_truncation_at_4000_chars() {
    let long_content = "x".repeat(200);
    let mut messages = Vec::new();
    messages.push(make_msg("user", "Do something"));
    for i in 0..50 {
        messages.push(make_tool_msg(&format!("tool_{i}"), &long_content));
    }
    let summary = ContextCompactor::fallback_summary(&messages);
    // Should stop before including all 50 tool results
    let action_count = summary.matches("- tool_").count();
    assert!(action_count < 50);
    assert!(action_count > 0);
}

#[test]
fn test_fallback_summary_empty_messages() {
    let summary = ContextCompactor::fallback_summary(&[]);
    assert!(summary.contains("Unknown"));
    assert!(summary.contains("None recorded"));
    assert!(summary.contains("No assistant response recorded"));
}

#[test]
fn test_fallback_summary_skips_system_messages_for_goal() {
    let messages = vec![
        make_msg("user", "[SYSTEM] You are an AI assistant"),
        make_msg("user", "Help me with X"),
        make_msg("assistant", "Sure"),
    ];
    let summary = ContextCompactor::fallback_summary(&messages);
    assert!(summary.contains("Help me with X"));
    assert!(!summary.contains("[SYSTEM]"));
}

#[test]
fn test_extract_content_string() {
    let msg = make_msg("user", "hello");
    assert_eq!(ContextCompactor::extract_content(&msg), "hello");
}

#[test]
fn test_extract_content_array() {
    let msg = make_array_content_msg("user", "multi-part content");
    assert_eq!(
        ContextCompactor::extract_content(&msg),
        "multi-part content"
    );
}

#[test]
fn test_extract_content_missing() {
    let msg = ApiMessage::new();
    assert_eq!(ContextCompactor::extract_content(&msg), "");
}

#[test]
fn test_sanitize_for_summarization_handles_array_content() {
    let messages = vec![
        make_array_content_msg("user", "array content message"),
        make_msg("assistant", "string content message"),
    ];
    let result = ContextCompactor::sanitize_for_summarization(&messages);
    assert!(result.contains("array content message"));
    assert!(result.contains("string content message"));
}
