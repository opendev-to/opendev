//! Session index management for fast metadata lookups.
//!
//! The index is a cached JSON file (`sessions-index.json`) that stores
//! session metadata for quick listing without reading each session file.

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use opendev_models::{Session, SessionMetadata};

use crate::file_locks::with_file_lock;

/// Current index file version.
pub const INDEX_VERSION: u32 = 1;

/// Index file name.
pub const SESSIONS_INDEX_FILE_NAME: &str = "sessions-index.json";

/// On-disk index format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexFile {
    pub version: u32,
    pub entries: Vec<IndexEntry>,
}

/// A single entry in the sessions index (camelCase for Python compatibility).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub session_id: String,
    pub created: String,
    pub modified: String,
    #[serde(default)]
    pub message_count: usize,
    #[serde(default)]
    pub total_tokens: u64,
    pub title: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub working_directory: Option<String>,
    #[serde(default)]
    pub has_session_model: bool,
    #[serde(default = "default_channel")]
    pub channel: String,
    #[serde(default)]
    pub channel_user_id: String,
    pub thread_id: Option<String>,
    pub owner_id: Option<String>,
    #[serde(default)]
    pub summary_additions: u64,
    #[serde(default)]
    pub summary_deletions: u64,
    #[serde(default)]
    pub summary_files: u64,
    pub time_archived: Option<String>,
}

fn default_channel() -> String {
    "cli".to_string()
}

/// Session index operations.
pub struct SessionIndex {
    session_dir: PathBuf,
}

impl SessionIndex {
    pub fn new(session_dir: PathBuf) -> Self {
        Self { session_dir }
    }

    /// Path to the sessions index file.
    pub fn index_path(&self) -> PathBuf {
        self.session_dir.join(SESSIONS_INDEX_FILE_NAME)
    }

    /// Read the sessions index file.
    ///
    /// Returns `None` if missing, corrupted, or wrong version.
    pub fn read_index(&self) -> Option<IndexFile> {
        let path = self.index_path();
        if !path.exists() {
            return None;
        }
        let data = std::fs::read_to_string(&path).ok()?;
        let index: IndexFile = serde_json::from_str(&data).ok()?;
        if index.version != INDEX_VERSION {
            return None;
        }
        Some(index)
    }

    /// Atomically write the sessions index file.
    ///
    /// Uses exclusive lock for cross-process safety.
    pub fn write_index(&self, entries: &[IndexEntry]) -> std::io::Result<()> {
        let data = IndexFile {
            version: INDEX_VERSION,
            entries: entries.to_vec(),
        };

        let index_path = self.index_path();
        with_file_lock(&index_path, Duration::from_secs(10), || {
            // Write to temp file then rename (atomic on POSIX)
            let tmp_path = self.session_dir.join(".sessions-index-tmp.json");
            let content = serde_json::to_string_pretty(&data).map_err(std::io::Error::other)?;
            std::fs::write(&tmp_path, content)?;
            std::fs::rename(&tmp_path, &index_path)?;
            Ok(())
        })?
    }

    /// Convert a Session to an index entry.
    pub fn session_to_entry(session: &Session) -> IndexEntry {
        IndexEntry {
            session_id: session.id.clone(),
            created: session.created_at.to_rfc3339(),
            modified: session.updated_at.to_rfc3339(),
            message_count: session.messages.len(),
            total_tokens: session.total_tokens(),
            title: session
                .metadata
                .get("title")
                .and_then(|v| v.as_str())
                .map(String::from),
            summary: session
                .metadata
                .get("summary")
                .and_then(|v| v.as_str())
                .map(String::from),
            tags: session
                .metadata
                .get("tags")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            working_directory: session.working_directory.clone(),
            has_session_model: session
                .metadata
                .get("session_model")
                .map(|v| !v.is_null())
                .unwrap_or(false),
            channel: session.channel.clone(),
            channel_user_id: session.channel_user_id.clone(),
            thread_id: session.thread_id.clone(),
            owner_id: session.owner_id.clone(),
            summary_additions: session.summary_additions(),
            summary_deletions: session.summary_deletions(),
            summary_files: session.summary_files() as u64,
            time_archived: session.time_archived.map(|t| t.to_rfc3339()),
        }
    }

    /// Convert an index entry to SessionMetadata.
    pub fn entry_to_metadata(entry: &IndexEntry) -> SessionMetadata {
        SessionMetadata {
            id: entry.session_id.clone(),
            created_at: DateTime::parse_from_rfc3339(&entry.created)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&entry.modified)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            message_count: entry.message_count,
            total_tokens: entry.total_tokens,
            title: entry.title.clone(),
            summary: entry.summary.clone(),
            tags: entry.tags.clone(),
            working_directory: entry.working_directory.clone(),
            has_session_model: entry.has_session_model,
            owner_id: entry.owner_id.clone(),
            summary_additions: entry.summary_additions,
            summary_deletions: entry.summary_deletions,
            summary_files: entry.summary_files,
            channel: entry.channel.clone(),
            channel_user_id: entry.channel_user_id.clone(),
            thread_id: entry.thread_id.clone(),
        }
    }

    /// Upsert a single session entry in the index.
    pub fn upsert_entry(&self, session: &Session) -> std::io::Result<()> {
        let new_entry = Self::session_to_entry(session);
        let mut entries = self.read_index().map(|idx| idx.entries).unwrap_or_default();

        // Replace existing or append
        if let Some(pos) = entries.iter().position(|e| e.session_id == session.id) {
            entries[pos] = new_entry;
        } else {
            entries.push(new_entry);
        }

        self.write_index(&entries)
    }

    /// Remove a single session entry from the index.
    pub fn remove_entry(&self, session_id: &str) -> std::io::Result<()> {
        if let Some(index) = self.read_index() {
            let entries: Vec<IndexEntry> = index
                .entries
                .into_iter()
                .filter(|e| e.session_id != session_id)
                .collect();
            self.write_index(&entries)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
