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

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use opendev_models::{ChatMessage, Role, Session};

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
    /// The rusqlite connection.
    connection: Connection,
}

impl SqliteSessionStore {
    /// Open (or create) a SQLite session store at the given path.
    ///
    /// Creates the database file and schema tables if they don't exist.
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, String> {
        let db_path = db_path.as_ref().to_path_buf();

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open SQLite database: {e}"))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )
        .map_err(|e| format!("Failed to set PRAGMA: {e}"))?;

        conn.execute_batch(CREATE_SESSIONS_TABLE)
            .map_err(|e| format!("Failed to create sessions table: {e}"))?;
        conn.execute_batch(CREATE_MESSAGES_TABLE)
            .map_err(|e| format!("Failed to create messages table: {e}"))?;
        for idx_sql in CREATE_INDEXES {
            conn.execute_batch(idx_sql)
                .map_err(|e| format!("Failed to create index: {e}"))?;
        }

        Ok(Self {
            db_path,
            connection: conn,
        })
    }

    /// Get the database file path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Save a session and all its messages to the database.
    ///
    /// Uses an `INSERT OR REPLACE` to handle both create and update.
    pub fn save_session(&self, session: &Session) -> Result<(), String> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(|e| format!("Transaction start failed: {e}"))?;

        let title: Option<String> = session
            .metadata
            .get("title")
            .and_then(|v| v.as_str())
            .map(String::from);

        let metadata_json =
            serde_json::to_string(&session.metadata).unwrap_or_else(|_| "{}".to_string());

        let time_archived = session
            .time_archived
            .map(|t| t.to_rfc3339());

        tx.execute(
            "INSERT OR REPLACE INTO sessions
             (id, created_at, updated_at, title, working_directory,
              parent_id, channel, channel_user_id, time_archived, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                session.id,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
                title,
                session.working_directory,
                session.parent_id,
                session.channel,
                session.channel_user_id,
                time_archived,
                metadata_json,
            ],
        )
        .map_err(|e| format!("Failed to insert session: {e}"))?;

        // Delete existing messages and re-insert
        tx.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session.id],
        )
        .map_err(|e| format!("Failed to delete old messages: {e}"))?;

        for (seq, msg) in session.messages.iter().enumerate() {
            Self::insert_message(&tx, &session.id, seq, msg)?;
        }

        tx.commit().map_err(|e| format!("Commit failed: {e}"))?;
        Ok(())
    }

    /// Load a session and its messages from the database.
    pub fn load_session(&self, session_id: &str) -> Result<Session, String> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT id, created_at, updated_at, title, working_directory,
                        parent_id, channel, channel_user_id, time_archived, metadata_json
                 FROM sessions WHERE id = ?1",
            )
            .map_err(|e| format!("Failed to prepare session query: {e}"))?;

        let session_row = stmt
            .query_row(params![session_id], |row| {
                let id: String = row.get(0)?;
                let created_at: String = row.get(1)?;
                let updated_at: String = row.get(2)?;
                let title: Option<String> = row.get(3)?;
                let working_directory: Option<String> = row.get(4)?;
                let parent_id: Option<String> = row.get(5)?;
                let channel: String = row.get(6)?;
                let channel_user_id: String = row.get(7)?;
                let time_archived: Option<String> = row.get(8)?;
                let metadata_json: String = row.get(9)?;

                Ok((
                    id,
                    created_at,
                    updated_at,
                    title,
                    working_directory,
                    parent_id,
                    channel,
                    channel_user_id,
                    time_archived,
                    metadata_json,
                ))
            })
            .map_err(|e| format!("Session not found: {e}"))?;

        let (
            id,
            created_at_str,
            updated_at_str,
            title,
            working_directory,
            parent_id,
            channel,
            channel_user_id,
            time_archived_str,
            metadata_json,
        ) = session_row;

        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map_err(|e| format!("Invalid created_at: {e}"))?
            .with_timezone(&chrono::Utc);
        let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_at_str)
            .map_err(|e| format!("Invalid updated_at: {e}"))?
            .with_timezone(&chrono::Utc);
        let time_archived = time_archived_str
            .map(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .map_err(|e| format!("Invalid time_archived: {e}"))
            })
            .transpose()?;

        let mut metadata: HashMap<String, serde_json::Value> =
            serde_json::from_str(&metadata_json).unwrap_or_default();
        if let Some(t) = &title {
            metadata.insert("title".to_string(), serde_json::Value::String(t.clone()));
        }

        let messages = self.load_messages(session_id)?;

        let mut session = Session::new();
        session.id = id;
        session.created_at = created_at;
        session.updated_at = updated_at;
        session.working_directory = working_directory;
        session.parent_id = parent_id;
        session.channel = channel;
        session.channel_user_id = channel_user_id;
        session.time_archived = time_archived;
        session.metadata = metadata;
        session.messages = messages;

        Ok(session)
    }

    /// Delete a session and its messages from the database.
    pub fn delete_session(&self, session_id: &str) -> Result<(), String> {
        // Messages are cascade-deleted via ON DELETE CASCADE.
        self.connection
            .execute("DELETE FROM sessions WHERE id = ?1", params![session_id])
            .map_err(|e| format!("Failed to delete session: {e}"))?;
        Ok(())
    }

    /// List all session IDs, ordered by most recently updated.
    pub fn list_session_ids(&self) -> Result<Vec<String>, String> {
        let mut stmt = self
            .connection
            .prepare("SELECT id FROM sessions ORDER BY updated_at DESC")
            .map_err(|e| format!("Failed to prepare list query: {e}"))?;

        let ids = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("Failed to list sessions: {e}"))?
            .collect::<Result<Vec<String>, _>>()
            .map_err(|e| format!("Failed to collect session IDs: {e}"))?;

        Ok(ids)
    }

    /// Search messages across all sessions for a query string.
    pub fn search_messages(&self, query: &str) -> Result<Vec<(String, Vec<usize>)>, String> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT session_id, seq FROM messages
                 WHERE content LIKE '%' || ?1 || '%'
                 ORDER BY session_id, seq",
            )
            .map_err(|e| format!("Failed to prepare search query: {e}"))?;

        let rows: Vec<(String, usize)> = stmt
            .query_map(params![query], |row| {
                let session_id: String = row.get(0)?;
                let seq: usize = row.get(1)?;
                Ok((session_id, seq))
            })
            .map_err(|e| format!("Failed to search messages: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect search results: {e}"))?;

        // Group by session_id
        let mut grouped: Vec<(String, Vec<usize>)> = Vec::new();
        for (session_id, seq) in rows {
            if let Some(last) = grouped.last_mut() {
                if last.0 == session_id {
                    last.1.push(seq);
                    continue;
                }
            }
            grouped.push((session_id, vec![seq]));
        }

        Ok(grouped)
    }

    /// Append a single message to a session without rewriting all messages.
    ///
    /// This is a key advantage of SQLite over the JSON file backend:
    /// appending a message is O(1) instead of O(n).
    pub fn append_message(&self, session_id: &str, message: &ChatMessage) -> Result<(), String> {
        let seq: i64 = self
            .connection
            .query_row(
                "SELECT COALESCE(MAX(seq), -1) + 1 FROM messages WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to get next seq: {e}"))?;

        Self::insert_message(&self.connection, session_id, seq as usize, message)?;
        Ok(())
    }

    /// Insert a single message row using the given connection (or transaction).
    fn insert_message(
        conn: &Connection,
        session_id: &str,
        seq: usize,
        msg: &ChatMessage,
    ) -> Result<(), String> {
        let metadata_json =
            serde_json::to_string(&msg.metadata).unwrap_or_else(|_| "{}".to_string());
        let tool_calls_json =
            serde_json::to_string(&msg.tool_calls).unwrap_or_else(|_| "[]".to_string());
        let tokens: Option<i64> = msg.tokens.map(|t| t as i64);

        conn.execute(
            "INSERT INTO messages
             (session_id, seq, role, content, timestamp,
              metadata_json, tool_calls_json, tokens, thinking_trace, reasoning_content)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                session_id,
                seq as i64,
                msg.role.to_string(),
                msg.content,
                msg.timestamp.to_rfc3339(),
                metadata_json,
                tool_calls_json,
                tokens,
                msg.thinking_trace,
                msg.reasoning_content,
            ],
        )
        .map_err(|e| format!("Failed to insert message at seq {seq}: {e}"))?;

        Ok(())
    }

    /// Load all messages for a session, ordered by seq.
    fn load_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>, String> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT role, content, timestamp, metadata_json, tool_calls_json,
                        tokens, thinking_trace, reasoning_content
                 FROM messages WHERE session_id = ?1 ORDER BY seq ASC",
            )
            .map_err(|e| format!("Failed to prepare messages query: {e}"))?;

        let messages = stmt
            .query_map(params![session_id], |row| {
                let role_str: String = row.get(0)?;
                let content: String = row.get(1)?;
                let timestamp_str: String = row.get(2)?;
                let metadata_json: String = row.get(3)?;
                let tool_calls_json: String = row.get(4)?;
                let tokens: Option<i64> = row.get(5)?;
                let thinking_trace: Option<String> = row.get(6)?;
                let reasoning_content: Option<String> = row.get(7)?;

                Ok((
                    role_str,
                    content,
                    timestamp_str,
                    metadata_json,
                    tool_calls_json,
                    tokens,
                    thinking_trace,
                    reasoning_content,
                ))
            })
            .map_err(|e| format!("Failed to query messages: {e}"))?;

        let mut result = Vec::new();
        for row in messages {
            let (
                role_str,
                content,
                timestamp_str,
                metadata_json,
                tool_calls_json,
                tokens,
                thinking_trace,
                reasoning_content,
            ) = row.map_err(|e| format!("Failed to read message row: {e}"))?;

            let role: Role = role_str
                .parse()
                .map_err(|_| format!("Invalid role: {role_str}"))?;
            let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                .map_err(|e| format!("Invalid timestamp: {e}"))?
                .with_timezone(&chrono::Utc);
            let metadata: HashMap<String, serde_json::Value> =
                serde_json::from_str(&metadata_json).unwrap_or_default();
            let tool_calls = serde_json::from_str(&tool_calls_json).unwrap_or_default();

            result.push(ChatMessage {
                role,
                content,
                timestamp,
                metadata,
                tool_calls,
                tokens: tokens.map(|t| t as u64),
                thinking_trace,
                reasoning_content,
                token_usage: None,
                provenance: None,
            });
        }

        Ok(result)
    }
}

#[cfg(test)]
#[path = "sqlite_store_tests.rs"]
mod tests;
