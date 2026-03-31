use super::*;

#[test]
fn test_session_new() {
    let session = Session::new();
    assert!(!session.id.is_empty());
    assert_eq!(session.channel, "cli");
    assert_eq!(session.chat_type, "direct");
    assert!(!session.is_archived());
}

#[test]
fn test_session_roundtrip() {
    let session = Session::new();
    let json = serde_json::to_string(&session).unwrap();
    let deserialized: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, session.id);
    assert_eq!(deserialized.channel, "cli");
}

#[test]
fn test_archive_unarchive() {
    let mut session = Session::new();
    assert!(!session.is_archived());

    session.archive();
    assert!(session.is_archived());

    session.unarchive();
    assert!(!session.is_archived());
}

#[test]
fn test_generate_slug() {
    let session = Session::new();

    assert_eq!(
        session.generate_slug(Some("Hello World Test")),
        "hello-world-test"
    );
    assert_eq!(
        session.generate_slug(Some("Special @#$ Characters!")),
        "special-characters"
    );
    // Empty title falls back to session ID prefix
    let slug = session.generate_slug(Some(""));
    assert_eq!(slug.len(), session.id.len().min(8));
}

#[test]
fn test_file_changes_summary() {
    let mut session = Session::new();

    session.add_file_change(FileChange {
        change_type: FileChangeType::Created,
        file_path: "src/main.rs".to_string(),
        lines_added: 50,
        ..FileChange::new(FileChangeType::Created, "src/main.rs".to_string())
    });

    session.add_file_change(FileChange {
        change_type: FileChangeType::Modified,
        file_path: "src/lib.rs".to_string(),
        lines_added: 10,
        lines_removed: 5,
        ..FileChange::new(FileChangeType::Modified, "src/lib.rs".to_string())
    });

    let summary = session.get_file_changes_summary();
    assert_eq!(summary.total, 2);
    assert_eq!(summary.created, 1);
    assert_eq!(summary.modified, 1);
    assert_eq!(summary.total_lines_added, 60);
    assert_eq!(summary.total_lines_removed, 5);
    assert_eq!(summary.net_lines, 55);
}

/// Verify that a JSON string matching the Python session format can be
/// deserialized into the Rust Session struct.  This guards against
/// serialization drift between the Python and Rust codebases.
#[test]
fn test_python_session_compat() {
    let python_json = r#"{
        "id": "a1b2c3d4e5f6",
        "created_at": "2025-06-15T10:30:00Z",
        "updated_at": "2025-06-15T11:45:00Z",
        "messages": [
            {
                "role": "user",
                "content": "Hello from Python",
                "timestamp": "2025-06-15T10:30:00Z",
                "metadata": {},
                "tool_calls": [],
                "tokens": null
            },
            {
                "role": "assistant",
                "content": "Hi! How can I help?",
                "timestamp": "2025-06-15T10:30:05Z",
                "metadata": {"model": "gpt-4"},
                "tool_calls": [
                    {
                        "id": "tc-001",
                        "name": "read_file",
                        "parameters": {"path": "/tmp/test.py"},
                        "result": "print('hello')",
                        "result_summary": "Read 1 line",
                        "timestamp": "2025-06-15T10:30:03Z",
                        "approved": true,
                        "error": null,
                        "nested_tool_calls": []
                    }
                ],
                "tokens": 42,
                "thinking_trace": "I should read the file first.",
                "reasoning_content": null,
                "token_usage": {"prompt_tokens": 100, "completion_tokens": 42},
                "provenance": null
            }
        ],
        "context_files": ["src/main.py"],
        "working_directory": "/home/user/project",
        "metadata": {"title": "Test session", "tags": ["rust", "python"]},
        "file_changes": [
            {
                "id": "fc-001",
                "type": "modified",
                "file_path": "src/main.py",
                "timestamp": "2025-06-15T11:00:00Z",
                "lines_added": 10,
                "lines_removed": 3
            }
        ],
        "channel": "cli",
        "chat_type": "direct",
        "channel_user_id": "",
        "thread_id": null,
        "delivery_context": {},
        "last_activity": "2025-06-15T11:45:00Z",
        "workspace_confirmed": true,
        "owner_id": "user-123",
        "parent_id": null,
        "subagent_sessions": {"tc-agent-1": "child-sess-1"},
        "time_archived": null,
        "slug": "test-session"
    }"#;

    let session: Session = serde_json::from_str(python_json)
        .expect("Python session JSON must deserialize into Rust Session");

    assert_eq!(session.id, "a1b2c3d4e5f6");
    assert_eq!(session.channel, "cli");
    assert_eq!(session.chat_type, "direct");
    assert_eq!(session.messages.len(), 2);
    assert_eq!(session.messages[0].role, crate::message::Role::User);
    assert_eq!(session.messages[0].content, "Hello from Python");
    assert_eq!(session.messages[1].tool_calls.len(), 1);
    assert_eq!(session.messages[1].tool_calls[0].name, "read_file");
    assert!(session.messages[1].thinking_trace.is_some());
    assert_eq!(session.context_files, vec!["src/main.py"]);
    assert_eq!(
        session.working_directory.as_deref(),
        Some("/home/user/project")
    );
    assert!(session.workspace_confirmed);
    assert_eq!(session.owner_id.as_deref(), Some("user-123"));
    assert_eq!(session.file_changes.len(), 1);
    assert_eq!(session.file_changes[0].lines_added, 10);
    assert_eq!(session.file_changes[0].lines_removed, 3);
    assert_eq!(
        session
            .subagent_sessions
            .get("tc-agent-1")
            .map(String::as_str),
        Some("child-sess-1")
    );
    assert_eq!(session.slug.as_deref(), Some("test-session"));
    assert!(session.last_activity.is_some());
    assert!(!session.is_archived());

    // Round-trip: serialize back and re-deserialize
    let reserialized = serde_json::to_string(&session).unwrap();
    let roundtrip: Session = serde_json::from_str(&reserialized).unwrap();
    assert_eq!(roundtrip.id, session.id);
    assert_eq!(roundtrip.messages.len(), session.messages.len());
}

/// Verify that a minimal Python session (only required fields) deserializes
/// correctly, with all optional/default fields populated.
#[test]
fn test_python_minimal_session_compat() {
    let minimal_json = r#"{}"#;
    let session: Session = serde_json::from_str(minimal_json)
        .expect("Empty JSON object must deserialize with defaults");

    assert!(!session.id.is_empty());
    assert_eq!(session.channel, "cli");
    assert_eq!(session.chat_type, "direct");
    assert!(session.messages.is_empty());
    assert!(session.file_changes.is_empty());
    assert!(session.working_directory.is_none());
    assert!(!session.workspace_confirmed);
}
