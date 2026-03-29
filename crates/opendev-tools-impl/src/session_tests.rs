use super::*;
use opendev_models::{ChatMessage, Role};
use tempfile::TempDir;

fn make_message(role_str: &str, content: &str) -> ChatMessage {
    let role = match role_str {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "system" => Role::System,
        _ => Role::User,
    };
    ChatMessage {
        role,
        content: content.to_string(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: Vec::new(),
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    }
}

/// Helper: create a SessionManager in a temp dir and add a session with messages.
fn create_test_session(
    dir: &std::path::Path,
    id: &str,
    title: &str,
    messages: Vec<(&str, &str)>,
) {
    let mut manager = SessionManager::new(dir.to_path_buf()).unwrap();
    let session = manager.create_session();
    // Override the auto-generated ID
    let session_id = session.id.clone();

    // We need to manipulate the session directly
    if let Some(s) = manager.current_session_mut() {
        s.id = id.to_string();
        s.metadata.insert(
            "title".to_string(),
            serde_json::Value::String(title.to_string()),
        );
        for (role, content) in messages {
            s.messages.push(make_message(role, content));
        }
    }

    // Save with the correct ID — need to save via the manager
    if let Some(s) = manager.current_session() {
        manager.save_session(s).unwrap();
    }

    // Clean up the auto-generated session file if different from our ID
    if session_id != id {
        let _ = std::fs::remove_file(dir.join(format!("{session_id}.json")));
        let _ = std::fs::remove_file(dir.join(format!("{session_id}.jsonl")));
    }
}

#[test]
fn test_list_empty() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let result = action_list(&manager, &HashMap::new(), None);
    assert!(result.success);
    assert!(result.output.unwrap().contains("No past sessions"));
}

#[test]
fn test_list_sessions() {
    let tmp = TempDir::new().unwrap();
    create_test_session(
        tmp.path(),
        "sess-001",
        "First session",
        vec![("user", "hello"), ("assistant", "hi there")],
    );
    create_test_session(
        tmp.path(),
        "sess-002",
        "Second session",
        vec![("user", "how are you")],
    );

    let manager = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let result = action_list(&manager, &HashMap::new(), None);
    assert!(result.success);
    let out = result.output.unwrap();
    assert!(out.contains("sess-001") || out.contains("sess-002"));
}

#[test]
fn test_list_excludes_current() {
    let tmp = TempDir::new().unwrap();
    create_test_session(
        tmp.path(),
        "current-sess",
        "Current",
        vec![("user", "test")],
    );
    create_test_session(tmp.path(), "other-sess", "Other", vec![("user", "test")]);

    let manager = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let result = action_list(&manager, &HashMap::new(), Some("current-sess"));
    assert!(result.success);
    let out = result.output.unwrap();
    assert!(!out.contains("current-sess"));
    assert!(out.contains("other-sess"));
}

#[test]
fn test_read_pagination() {
    let tmp = TempDir::new().unwrap();
    let messages: Vec<(&str, &str)> = (0..10)
        .map(|i| {
            if i % 2 == 0 {
                ("user", "question")
            } else {
                ("assistant", "answer")
            }
        })
        .collect();
    create_test_session(tmp.path(), "paged-sess", "Paged", messages);

    let manager = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut args = HashMap::new();
    args.insert("session_id".to_string(), serde_json::json!("paged-sess"));
    args.insert("limit".to_string(), serde_json::json!(3));
    args.insert("offset".to_string(), serde_json::json!(0));

    let result = action_read(&manager, &args, None);
    assert!(result.success);
    let out = result.output.unwrap();
    assert!(out.contains("[1]"));
    assert!(out.contains("[3]"));
    // Should not contain message 4
    assert!(!out.contains("[4]"));
}

#[test]
fn test_read_current_blocked() {
    let tmp = TempDir::new().unwrap();
    create_test_session(tmp.path(), "my-sess", "Mine", vec![("user", "hi")]);

    let manager = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut args = HashMap::new();
    args.insert("session_id".to_string(), serde_json::json!("my-sess"));

    let result = action_read(&manager, &args, Some("my-sess"));
    assert!(result.success); // It's an ok() with guidance, not a fail()
    assert!(result.output.unwrap().contains("current session"));
}

#[test]
fn test_search() {
    let tmp = TempDir::new().unwrap();
    create_test_session(
        tmp.path(),
        "search-sess",
        "Searchable",
        vec![
            ("user", "How do I configure the database?"),
            ("assistant", "You can configure the database in config.yaml"),
        ],
    );

    let manager = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut args = HashMap::new();
    args.insert("query".to_string(), serde_json::json!("database"));

    let result = action_search(&manager, &args);
    assert!(result.success);
    let out = result.output.unwrap();
    assert!(out.contains("search-sess"));
}

#[test]
fn test_info() {
    let tmp = TempDir::new().unwrap();
    create_test_session(
        tmp.path(),
        "info-sess",
        "Info Test",
        vec![("user", "hello"), ("assistant", "world")],
    );

    let manager = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut args = HashMap::new();
    args.insert("session_id".to_string(), serde_json::json!("info-sess"));

    let result = action_info(&manager, &args, None);
    assert!(result.success);
    let out = result.output.unwrap();
    assert!(out.contains("info-sess"));
    assert!(out.contains("Info Test"));
    assert!(out.contains("Messages: 2"));
}

#[test]
fn test_path_traversal() {
    assert!(validate_session_id("../etc/passwd").is_some());
    assert!(validate_session_id("foo/bar").is_some());
    assert!(validate_session_id("foo\\bar").is_some());
    assert!(validate_session_id("").is_some());
    assert!(validate_session_id("valid-session-id").is_none());
}

#[tokio::test]
async fn test_subagent_blocked() {
    let tool = PastSessionsTool;
    let mut args = HashMap::new();
    args.insert("action".to_string(), serde_json::json!("list"));

    let ctx = ToolContext::new("/tmp").with_subagent(true);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not available to subagents"));
}

#[test]
fn test_secrets_redacted() {
    let tmp = TempDir::new().unwrap();
    create_test_session(
        tmp.path(),
        "secret-sess",
        "Secret Test",
        vec![(
            "user",
            "My API key is sk-ant-api03-abcdefghij1234567890abcdefghij1234567890abcdefghij",
        )],
    );

    let manager = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut args = HashMap::new();
    args.insert("session_id".to_string(), serde_json::json!("secret-sess"));

    let result = action_read(&manager, &args, None);
    assert!(result.success);
    let out = result.output.unwrap();
    assert!(!out.contains("abcdefghij1234567890"));
    assert!(out.contains("[REDACTED]"));
}
