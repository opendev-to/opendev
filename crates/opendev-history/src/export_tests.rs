use super::*;
use chrono::Utc;
use opendev_models::ChatMessage;
use std::collections::HashMap;

fn make_msg(role: Role, content: &str) -> ChatMessage {
    ChatMessage {
        role,
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
fn test_export_empty_session() {
    let session = Session::new();
    let md = export_markdown(&session);
    assert!(md.contains("# Session Export"));
    assert!(md.contains("Untitled"));
    assert!(md.contains("Messages:** 0"));
}

#[test]
fn test_export_with_messages() {
    let mut session = Session::new();
    session
        .metadata
        .insert("title".to_string(), serde_json::json!("Test Session"));
    session.messages.push(make_msg(Role::User, "Hello there"));
    session
        .messages
        .push(make_msg(Role::Assistant, "Hi! How can I help?"));

    let md = export_markdown(&session);
    assert!(md.contains("**Title:** Test Session"));
    assert!(md.contains("## User (Turn 1)"));
    assert!(md.contains("Hello there"));
    assert!(md.contains("## Assistant (Turn 2)"));
    assert!(md.contains("Hi! How can I help?"));
}

#[test]
fn test_export_with_thinking_trace() {
    let mut session = Session::new();
    let mut msg = make_msg(Role::Assistant, "The answer is 42.");
    msg.thinking_trace = Some("Let me think about this...".to_string());
    session.messages.push(msg);

    let md = export_markdown(&session);
    assert!(md.contains("<details>"));
    assert!(md.contains("<summary>Thinking</summary>"));
    assert!(md.contains("Let me think about this..."));
    assert!(md.contains("</details>"));
    assert!(md.contains("The answer is 42."));
}

#[test]
fn test_export_with_tool_calls() {
    let mut session = Session::new();
    let mut msg = make_msg(Role::Assistant, "I'll read the file.");
    msg.tool_calls.push(opendev_models::ToolCall {
        id: "tc-1".to_string(),
        name: "read_file".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert("path".to_string(), serde_json::json!("/src/main.rs"));
            p
        },
        result: Some(serde_json::json!("fn main() {}")),
        result_summary: None,
        timestamp: Utc::now(),
        approved: true,
        error: None,
        nested_tool_calls: vec![],
    });
    session.messages.push(msg);

    let md = export_markdown(&session);
    assert!(md.contains("### Tool: `read_file`"));
    assert!(md.contains("```json"));
    assert!(md.contains("/src/main.rs"));
    assert!(md.contains("**Result:**"));
    assert!(md.contains("fn main() {}"));
}

#[test]
fn test_export_with_tool_error() {
    let mut session = Session::new();
    let mut msg = make_msg(Role::Assistant, "Trying to write.");
    msg.tool_calls.push(opendev_models::ToolCall {
        id: "tc-2".to_string(),
        name: "write_file".to_string(),
        parameters: HashMap::new(),
        result: None,
        result_summary: None,
        timestamp: Utc::now(),
        approved: false,
        error: Some("Permission denied".to_string()),
        nested_tool_calls: vec![],
    });
    session.messages.push(msg);

    let md = export_markdown(&session);
    assert!(md.contains("**Error:** Permission denied"));
}

#[test]
fn test_export_with_working_directory() {
    let mut session = Session::new();
    session.working_directory = Some("/home/user/project".to_string());

    let md = export_markdown(&session);
    assert!(md.contains("**Working Directory:** /home/user/project"));
}

#[test]
fn test_export_system_message() {
    let mut session = Session::new();
    session
        .messages
        .push(make_msg(Role::System, "Context loaded."));

    let md = export_markdown(&session);
    assert!(md.contains("## System (Turn 1)"));
    assert!(md.contains("Context loaded."));
}

#[test]
fn test_export_multi_turn_conversation() {
    let mut session = Session::new();
    session.messages.push(make_msg(Role::User, "Question 1"));
    session.messages.push(make_msg(Role::Assistant, "Answer 1"));
    session.messages.push(make_msg(Role::User, "Question 2"));
    session.messages.push(make_msg(Role::Assistant, "Answer 2"));

    let md = export_markdown(&session);
    assert!(md.contains("## User (Turn 1)"));
    assert!(md.contains("## Assistant (Turn 2)"));
    assert!(md.contains("## User (Turn 3)"));
    assert!(md.contains("## Assistant (Turn 4)"));
}
