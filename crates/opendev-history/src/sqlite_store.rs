//! SQLite-backed session storage.
//!
//! Provides [`SqliteSessionStore`] as an alternative to the default JSON
//! file-based session persistence.  SQLite offers better concurrent access,
//! indexing, and query capabilities for large session histories.
//!
//! # Schema
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS sessions (
//!     id              TEXT PRIMARY KEY,
//!     created_at      TEXT NOT NULL,        -- ISO-8601
//!     updated_at      TEXT NOT NULL,        -- ISO-8601
//!     title           TEXT,
//!     working_directory TEXT,
//!     parent_id       TEXT,
//!     channel         TEXT NOT NULL DEFAULT 'cli',
//!     channel_user_id TEXT NOT NULL DEFAULT '',
//!     time_archived   TEXT,                 -- NULL if not archived
//!     metadata_json   TEXT NOT NULL DEFAULT '{}'
//! );
//!
//! CREATE TABLE IF NOT EXISTS messages (
//!     id          INTEGER PRIMARY KEY AUTOINCREMENT,
//!     session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
//!     seq         INTEGER NOT NULL,         -- 0-based order within session
//!     role        TEXT NOT NULL,             -- 'user', 'assistant', 'system'
//!     content     TEXT NOT NULL,
//!     timestamp   TEXT NOT NULL,             -- ISO-8601
//!     metadata_json TEXT NOT NULL DEFAULT '{}',
//!     tool_calls_json TEXT NOT NULL DEFAULT '[]',
//!     tokens      INTEGER,
//!     thinking_trace TEXT,
//!     reasoning_content TEXT,
//!     UNIQUE(session_id, seq)
//! );
//!
//! CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
//! CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at DESC);
//! CREATE INDEX IF NOT EXISTS idx_sessions_channel ON sessions(channel);
//! ```
//!
//! # Status
//!
//! This module defines the schema and basic CRUD interface.  The actual
//! `rusqlite` dependency is not yet added to `Cargo.toml`, so the
//! implementation methods contain TODO markers.  Once `rusqlite` is
//! available in the workspace, fill in the method bodies.

use std::path::{Path, PathBuf};

use opendev_models::{ChatMessage, Session};

/// SQL statements for creating the schema.
pub const CREATE_SESSIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id              TEXT PRIMARY KEY,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    title           TEXT,
    working_directory TEXT,
    parent_id       TEXT,
    channel         TEXT NOT NULL DEFAULT 'cli',
    channel_user_id TEXT NOT NULL DEFAULT '',
    time_archived   TEXT,
    metadata_json   TEXT NOT NULL DEFAULT '{}'
)
"#;

pub const CREATE_MESSAGES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS messages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    role            TEXT NOT NULL,
    content         TEXT NOT NULL,
    timestamp       TEXT NOT NULL,
    metadata_json   TEXT NOT NULL DEFAULT '{}',
    tool_calls_json TEXT NOT NULL DEFAULT '[]',
    tokens          INTEGER,
    thinking_trace  TEXT,
    reasoning_content TEXT,
    UNIQUE(session_id, seq)
)
"#;

pub const CREATE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_channel ON sessions(channel)",
];

/// SQLite-backed session store.
///
/// Stores sessions and messages in a SQLite database file.
/// This is an alternative to the JSON file-based [`super::SessionManager`].
///
/// # Usage
///
/// ```ignore
/// let store = SqliteSessionStore::open("~/.opendev/sessions.db")?;
/// store.save_session(&session)?;
/// let loaded = store.load_session("session-id")?;
/// ```
#[derive(Debug)]
pub struct SqliteSessionStore {
    /// Path to the SQLite database file.
    db_path: PathBuf,
    // TODO: Add `rusqlite::Connection` field once the dependency is added.
    // connection: rusqlite::Connection,
}

impl SqliteSessionStore {
    /// Open (or create) a SQLite session store at the given path.
    ///
    /// Creates the database file and schema tables if they don't exist.
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, String> {
        let db_path = db_path.as_ref().to_path_buf();

        // TODO: Implement with rusqlite:
        // let conn = rusqlite::Connection::open(&db_path)
        //     .map_err(|e| format!("Failed to open SQLite database: {}", e))?;
        // conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        //     .map_err(|e| format!("Failed to set PRAGMA: {}", e))?;
        // conn.execute(CREATE_SESSIONS_TABLE, [])
        //     .map_err(|e| format!("Failed to create sessions table: {}", e))?;
        // conn.execute(CREATE_MESSAGES_TABLE, [])
        //     .map_err(|e| format!("Failed to create messages table: {}", e))?;
        // for idx_sql in CREATE_INDEXES {
        //     conn.execute(idx_sql, [])
        //         .map_err(|e| format!("Failed to create index: {}", e))?;
        // }

        Ok(Self {
            db_path,
            // connection: conn,
        })
    }

    /// Get the database file path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Save a session and all its messages to the database.
    ///
    /// Uses an `INSERT OR REPLACE` to handle both create and update.
    pub fn save_session(&self, _session: &Session) -> Result<(), String> {
        // TODO: Implement with rusqlite:
        // let tx = self.connection.transaction()
        //     .map_err(|e| format!("Transaction start failed: {}", e))?;
        //
        // tx.execute(
        //     "INSERT OR REPLACE INTO sessions
        //      (id, created_at, updated_at, title, working_directory,
        //       parent_id, channel, channel_user_id, time_archived, metadata_json)
        //      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        //     params![...],
        // )?;
        //
        // tx.execute("DELETE FROM messages WHERE session_id = ?1", [&session.id])?;
        //
        // for (seq, msg) in session.messages.iter().enumerate() {
        //     tx.execute("INSERT INTO messages ...", params![...])?;
        // }
        //
        // tx.commit().map_err(|e| format!("Commit failed: {}", e))?;
        Err("SQLite backend not yet implemented (rusqlite dependency needed)".to_string())
    }

    /// Load a session and its messages from the database.
    pub fn load_session(&self, _session_id: &str) -> Result<Session, String> {
        // TODO: Implement with rusqlite:
        // SELECT ... FROM sessions WHERE id = ?1
        // SELECT ... FROM messages WHERE session_id = ?1 ORDER BY seq ASC
        Err("SQLite backend not yet implemented (rusqlite dependency needed)".to_string())
    }

    /// Delete a session and its messages from the database.
    pub fn delete_session(&self, _session_id: &str) -> Result<(), String> {
        // TODO: Implement with rusqlite:
        // DELETE FROM sessions WHERE id = ?1
        // Messages are cascade-deleted via ON DELETE CASCADE.
        Err("SQLite backend not yet implemented (rusqlite dependency needed)".to_string())
    }

    /// List all session IDs, ordered by most recently updated.
    pub fn list_session_ids(&self) -> Result<Vec<String>, String> {
        // TODO: Implement with rusqlite:
        // SELECT id FROM sessions ORDER BY updated_at DESC
        Err("SQLite backend not yet implemented (rusqlite dependency needed)".to_string())
    }

    /// Search messages across all sessions for a query string.
    pub fn search_messages(&self, _query: &str) -> Result<Vec<(String, Vec<usize>)>, String> {
        // TODO: Implement with rusqlite:
        // SELECT session_id, seq FROM messages
        // WHERE content LIKE '%' || ?1 || '%'
        // ORDER BY session_id, seq
        Err("SQLite backend not yet implemented (rusqlite dependency needed)".to_string())
    }

    /// Append a single message to a session without rewriting all messages.
    ///
    /// This is a key advantage of SQLite over the JSON file backend:
    /// appending a message is O(1) instead of O(n).
    pub fn append_message(&self, _session_id: &str, _message: &ChatMessage) -> Result<(), String> {
        // TODO: Implement with rusqlite:
        // let seq = SELECT COALESCE(MAX(seq), -1) + 1 FROM messages WHERE session_id = ?1
        // INSERT INTO messages ...
        Err("SQLite backend not yet implemented (rusqlite dependency needed)".to_string())
    }
}

#[cfg(test)]
mod tests {
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
}
