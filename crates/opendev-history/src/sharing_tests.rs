use super::*;
use chrono::Utc;
use opendev_models::{ChatMessage, Role};
use std::collections::HashMap;
use std::path::Path;

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
fn test_anonymize_redacts_api_keys() {
    let mut session = Session::new();
    session.messages.push(make_msg(
        Role::User,
        "My key is sk-abcdefghijklmnopqrstuvwxyz123456 please use it",
    ));

    let anon = anonymize_session(&session);
    assert!(!anon.messages[0].content.contains("sk-"));
    assert!(anon.messages[0].content.contains("[REDACTED]"));
}

#[test]
fn test_anonymize_redacts_absolute_paths() {
    let mut session = Session::new();
    session.messages.push(make_msg(
        Role::User,
        "The file is at /Users/john/codes/project/main.rs",
    ));

    let anon = anonymize_session(&session);
    assert!(!anon.messages[0].content.contains("/Users/john"));
    assert!(anon.messages[0].content.contains("[PATH]"));
}

#[test]
fn test_anonymize_clears_working_directory() {
    let mut session = Session::new();
    session.working_directory = Some("/Users/john/codes/project".to_string());
    session.context_files = vec!["src/main.rs".to_string()];

    let anon = anonymize_session(&session);
    assert!(anon.working_directory.is_none());
    assert!(anon.context_files.is_empty());
}

#[test]
fn test_anonymize_redacts_bearer_tokens() {
    let mut session = Session::new();
    session.messages.push(make_msg(
        Role::Assistant,
        "Using Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature for auth",
    ));

    let anon = anonymize_session(&session);
    assert!(!anon.messages[0].content.contains("eyJhbGci"));
    assert!(anon.messages[0].content.contains("[REDACTED]"));
}

#[test]
fn test_anonymize_preserves_regular_content() {
    let mut session = Session::new();
    session
        .messages
        .push(make_msg(Role::User, "Hello, how are you?"));

    let anon = anonymize_session(&session);
    assert_eq!(anon.messages[0].content, "Hello, how are you?");
}

#[test]
fn test_render_session_html() {
    let mut session = Session::new();
    session
        .metadata
        .insert("title".into(), serde_json::json!("Test Session"));
    session.messages.push(make_msg(Role::User, "Hello <world>"));
    session
        .messages
        .push(make_msg(Role::Assistant, "Hi & welcome"));

    let html = render_session_html(&session);
    assert!(html.contains("<title>Test Session</title>"));
    assert!(html.contains("Hello &lt;world&gt;"));
    assert!(html.contains("Hi &amp; welcome"));
    assert!(html.contains("user"));
    assert!(html.contains("assistant"));
}

#[tokio::test]
async fn test_share_session_local_html() {
    let mut session = Session::new();
    session.id = "share-test-local".to_string();
    session.messages.push(make_msg(Role::User, "hello"));

    let result = share_session(&session, "").await;
    assert!(result.is_ok());
    let url = result.unwrap();
    assert!(url.starts_with("file://"));
    assert!(url.contains("share-test-local"));

    // Verify the file exists.
    let path = url.trim_start_matches("file://");
    assert!(Path::new(path).exists());
    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn test_share_session_remote_failure() {
    let mut session = Session::new();
    session.messages.push(make_msg(Role::User, "hello"));

    let result = share_session(&session, "http://127.0.0.1:1/nonexistent").await;
    assert!(result.is_err());
}

#[test]
fn test_html_escape() {
    assert_eq!(html_escape("<script>"), "&lt;script&gt;");
    assert_eq!(html_escape("a&b"), "a&amp;b");
    assert_eq!(html_escape("\"quote\""), "&quot;quote&quot;");
}

#[test]
fn test_redact_json_value() {
    let combined = SENSITIVE_PATTERNS.join("|");
    let re = Regex::new(&combined).unwrap();
    let mut val = serde_json::json!({
        "key": "sk-abcdefghijklmnopqrstuvwxyz123456",
        "nested": {
            "token": "Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.payload.sig"
        },
        "list": ["normal", "sk-aaaaaabbbbbbccccccdddddd"]
    });
    redact_json_value(&mut val, &re);
    assert!(val["key"].as_str().unwrap().contains("[REDACTED]"));
    assert!(
        val["nested"]["token"]
            .as_str()
            .unwrap()
            .contains("[REDACTED]")
    );
    assert_eq!(val["list"][0].as_str().unwrap(), "normal");
    assert!(val["list"][1].as_str().unwrap().contains("[REDACTED]"));
}
