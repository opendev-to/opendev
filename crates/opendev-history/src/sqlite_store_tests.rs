use super::*;

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
fn test_sqlite_store_open() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let store = SqliteSessionStore::open(&db_path).unwrap();
    assert_eq!(store.db_path(), db_path);
}

#[test]
fn test_sqlite_store_save_returns_not_implemented() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SqliteSessionStore::open(tmp.path().join("test.db")).unwrap();
    let session = Session::new();
    let result = store.save_session(&session);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not yet implemented"));
}

#[test]
fn test_sqlite_store_load_returns_not_implemented() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SqliteSessionStore::open(tmp.path().join("test.db")).unwrap();
    let result = store.load_session("some-id");
    assert!(result.is_err());
}

#[test]
fn test_sqlite_store_delete_returns_not_implemented() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SqliteSessionStore::open(tmp.path().join("test.db")).unwrap();
    let result = store.delete_session("some-id");
    assert!(result.is_err());
}

#[test]
fn test_sqlite_store_list_returns_not_implemented() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SqliteSessionStore::open(tmp.path().join("test.db")).unwrap();
    let result = store.list_session_ids();
    assert!(result.is_err());
}

#[test]
fn test_sqlite_store_search_returns_not_implemented() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SqliteSessionStore::open(tmp.path().join("test.db")).unwrap();
    let result = store.search_messages("query");
    assert!(result.is_err());
}

#[test]
fn test_sqlite_store_append_returns_not_implemented() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SqliteSessionStore::open(tmp.path().join("test.db")).unwrap();
    let msg = ChatMessage {
        role: opendev_models::Role::User,
        content: "hello".to_string(),
        timestamp: chrono::Utc::now(),
        metadata: std::collections::HashMap::new(),
        tool_calls: vec![],
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    };
    let result = store.append_message("id", &msg);
    assert!(result.is_err());
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
            "sessions schema missing column: {}",
            col
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
            "messages schema missing column: {}",
            col
        );
    }
}
