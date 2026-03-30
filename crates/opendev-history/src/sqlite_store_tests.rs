use super::*;

use std::collections::HashMap;

use chrono::Utc;
use opendev_models::{ChatMessage, Role, Session};

/// Helper to create a store backed by a temp directory.
fn temp_store() -> (tempfile::TempDir, SqliteSessionStore) {
    let tmp = tempfile::tempdir().unwrap();
    let tmp = tmp.into_path().canonicalize().unwrap();
    let tmp = tempfile::TempDir::new_in(tmp.parent().unwrap()).unwrap();
    let db_path = tmp.path().canonicalize().unwrap().join("test.db");
    let store = SqliteSessionStore::open(&db_path).unwrap();
    (tmp, store)
}

/// Helper to create a ChatMessage with given role and content.
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

/// Helper to create a Session with given id and messages.
fn make_session(id: &str, messages: Vec<ChatMessage>) -> Session {
    let mut session = Session::new();
    session.id = id.to_string();
    session.messages = messages;
    session.working_directory = Some("/tmp/test".to_string());
    session.channel = "cli".to_string();
    session
}

#[test]
fn test_schema_sql_is_valid_syntax() {
    assert!(CREATE_SESSIONS_TABLE.contains("CREATE TABLE"));
    assert!(CREATE_SESSIONS_TABLE.contains("sessions"));
    assert!(CREATE_SESSIONS_TABLE.contains("TEXT PRIMARY KEY"));

    assert!(CREATE_MESSAGES_TABLE.contains("CREATE TABLE"));
    assert!(CREATE_MESSAGES_TABLE.contains("messages"));
    assert!(CREATE_MESSAGES_TABLE.contains("REFERENCES sessions"));
    assert!(CREATE_MESSAGES_TABLE.contains("ON DELETE CASCADE"));
}

#[test]
fn test_create_indexes_count() {
    assert_eq!(CREATE_INDEXES.len(), 3);
    for sql in CREATE_INDEXES {
        assert!(sql.starts_with("CREATE INDEX"));
    }
}

#[test]
fn test_schema_has_required_columns() {
    for col in &[
        "id",
        "created_at",
        "updated_at",
        "title",
        "working_directory",
        "parent_id",
        "channel",
        "time_archived",
        "metadata_json",
    ] {
        assert!(
            CREATE_SESSIONS_TABLE.contains(col),
            "sessions schema missing column: {col}",
        );
    }

    for col in &[
        "session_id",
        "seq",
        "role",
        "content",
        "timestamp",
        "metadata_json",
        "tool_calls_json",
        "tokens",
        "thinking_trace",
        "reasoning_content",
    ] {
        assert!(
            CREATE_MESSAGES_TABLE.contains(col),
            "messages schema missing column: {col}",
        );
    }
}

#[test]
fn test_open_creates_tables() {
    let (_tmp, store) = temp_store();

    // Verify tables exist by querying sqlite_master
    let count: i32 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('sessions', 'messages')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 2, "Expected both sessions and messages tables");

    // Verify the db_path is set
    assert!(store.db_path().to_str().unwrap().contains("test.db"));
}

#[test]
fn test_save_and_load_session() {
    let (_tmp, store) = temp_store();

    let messages = vec![
        make_msg(Role::User, "Hello, world!"),
        make_msg(Role::Assistant, "Hi there!"),
    ];
    let session = make_session("sess-001", messages);

    store.save_session(&session).unwrap();

    let loaded = store.load_session("sess-001").unwrap();
    assert_eq!(loaded.id, "sess-001");
    assert_eq!(loaded.messages.len(), 2);
    assert_eq!(loaded.messages[0].role, Role::User);
    assert_eq!(loaded.messages[0].content, "Hello, world!");
    assert_eq!(loaded.messages[1].role, Role::Assistant);
    assert_eq!(loaded.messages[1].content, "Hi there!");
    assert_eq!(loaded.working_directory, Some("/tmp/test".to_string()));
    assert_eq!(loaded.channel, "cli");
}

#[test]
fn test_save_updates_existing() {
    let (_tmp, store) = temp_store();

    let session_v1 = make_session("sess-002", vec![make_msg(Role::User, "v1 message")]);
    store.save_session(&session_v1).unwrap();

    // Update with new messages
    let session_v2 = make_session(
        "sess-002",
        vec![
            make_msg(Role::User, "v2 message 1"),
            make_msg(Role::Assistant, "v2 message 2"),
            make_msg(Role::User, "v2 message 3"),
        ],
    );
    store.save_session(&session_v2).unwrap();

    let loaded = store.load_session("sess-002").unwrap();
    assert_eq!(loaded.messages.len(), 3);
    assert_eq!(loaded.messages[0].content, "v2 message 1");
    assert_eq!(loaded.messages[2].content, "v2 message 3");
}

#[test]
fn test_delete_session() {
    let (_tmp, store) = temp_store();

    let session = make_session("sess-del", vec![make_msg(Role::User, "to be deleted")]);
    store.save_session(&session).unwrap();

    // Verify it exists
    assert!(store.load_session("sess-del").is_ok());

    // Delete
    store.delete_session("sess-del").unwrap();

    // Verify it's gone
    assert!(store.load_session("sess-del").is_err());

    // Verify messages are cascade-deleted
    let msg_count: i32 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = 'sess-del'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(msg_count, 0);
}

#[test]
fn test_list_session_ids_ordered() {
    let (_tmp, store) = temp_store();

    // Create sessions with different updated_at times
    let mut s1 = make_session("sess-old", vec![]);
    s1.updated_at = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let mut s2 = make_session("sess-mid", vec![]);
    s2.updated_at = chrono::DateTime::parse_from_rfc3339("2025-06-15T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let mut s3 = make_session("sess-new", vec![]);
    s3.updated_at = chrono::DateTime::parse_from_rfc3339("2026-03-30T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    store.save_session(&s1).unwrap();
    store.save_session(&s2).unwrap();
    store.save_session(&s3).unwrap();

    let ids = store.list_session_ids().unwrap();
    assert_eq!(ids, vec!["sess-new", "sess-mid", "sess-old"]);
}

#[test]
fn test_search_messages() {
    let (_tmp, store) = temp_store();

    let s1 = make_session(
        "sess-a",
        vec![
            make_msg(Role::User, "hello rust world"),
            make_msg(Role::Assistant, "rust is great"),
        ],
    );
    let s2 = make_session(
        "sess-b",
        vec![
            make_msg(Role::User, "python is also nice"),
            make_msg(Role::Assistant, "but rust is faster"),
        ],
    );
    store.save_session(&s1).unwrap();
    store.save_session(&s2).unwrap();

    let results = store.search_messages("rust").unwrap();
    // sess-a has "rust" at seq 0 and 1, sess-b at seq 1
    assert_eq!(results.len(), 2);

    let sess_a_result = results.iter().find(|(id, _)| id == "sess-a").unwrap();
    assert_eq!(sess_a_result.1, vec![0, 1]);

    let sess_b_result = results.iter().find(|(id, _)| id == "sess-b").unwrap();
    assert_eq!(sess_b_result.1, vec![1]);

    // Search for something that doesn't exist
    let no_results = store.search_messages("nonexistent_xyz").unwrap();
    assert!(no_results.is_empty());
}

#[test]
fn test_append_message() {
    let (_tmp, store) = temp_store();

    let session = make_session("sess-app", vec![make_msg(Role::User, "first message")]);
    store.save_session(&session).unwrap();

    // Append a second message
    let msg2 = make_msg(Role::Assistant, "appended response");
    store.append_message("sess-app", &msg2).unwrap();

    let loaded = store.load_session("sess-app").unwrap();
    assert_eq!(loaded.messages.len(), 2);
    assert_eq!(loaded.messages[0].content, "first message");
    assert_eq!(loaded.messages[1].content, "appended response");
    assert_eq!(loaded.messages[1].role, Role::Assistant);

    // Append another
    let msg3 = make_msg(Role::User, "third message");
    store.append_message("sess-app", &msg3).unwrap();

    let loaded = store.load_session("sess-app").unwrap();
    assert_eq!(loaded.messages.len(), 3);
    assert_eq!(loaded.messages[2].content, "third message");
}

#[test]
fn test_load_nonexistent_session() {
    let (_tmp, store) = temp_store();

    let result = store.load_session("does-not-exist");
    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("not found"),
        "Error should indicate session not found"
    );
}

#[test]
fn test_save_session_with_metadata() {
    let (_tmp, store) = temp_store();

    let mut session = make_session("sess-meta", vec![]);
    session
        .metadata
        .insert("title".to_string(), serde_json::json!("My Session"));
    session
        .metadata
        .insert("custom_key".to_string(), serde_json::json!(42));

    store.save_session(&session).unwrap();
    let loaded = store.load_session("sess-meta").unwrap();

    assert_eq!(
        loaded.metadata.get("title").and_then(|v| v.as_str()),
        Some("My Session")
    );
    assert_eq!(
        loaded.metadata.get("custom_key").and_then(|v| v.as_i64()),
        Some(42)
    );
}

#[test]
fn test_save_session_with_thinking_trace() {
    let (_tmp, store) = temp_store();

    let mut msg = make_msg(Role::Assistant, "response");
    msg.thinking_trace = Some("I thought about this carefully".to_string());
    msg.reasoning_content = Some("Step 1: analyze. Step 2: respond.".to_string());
    msg.tokens = Some(150);

    let session = make_session("sess-think", vec![msg]);
    store.save_session(&session).unwrap();

    let loaded = store.load_session("sess-think").unwrap();
    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(
        loaded.messages[0].thinking_trace.as_deref(),
        Some("I thought about this carefully")
    );
    assert_eq!(
        loaded.messages[0].reasoning_content.as_deref(),
        Some("Step 1: analyze. Step 2: respond.")
    );
    assert_eq!(loaded.messages[0].tokens, Some(150));
}

#[test]
fn test_save_session_with_time_archived() {
    let (_tmp, store) = temp_store();

    let mut session = make_session("sess-archived", vec![]);
    session.time_archived = Some(Utc::now());

    store.save_session(&session).unwrap();
    let loaded = store.load_session("sess-archived").unwrap();
    assert!(loaded.time_archived.is_some());
}

#[test]
fn test_delete_nonexistent_is_ok() {
    let (_tmp, store) = temp_store();

    // Deleting a non-existent session should not error
    let result = store.delete_session("does-not-exist");
    assert!(result.is_ok());
}

#[test]
fn test_list_empty_store() {
    let (_tmp, store) = temp_store();

    let ids = store.list_session_ids().unwrap();
    assert!(ids.is_empty());
}
