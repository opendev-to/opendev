//! Application state snapshot for crash recovery (#97).
//!
//! Periodically serializes essential session state to a temp file so that
//! incomplete sessions can be detected and recovered on the next startup.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Filename used for the crash-recovery snapshot.
const SNAPSHOT_FILENAME: &str = "session_snapshot.json";

/// Subdirectory under `~/.opendev/data/` where snapshots live.
const SNAPSHOT_SUBDIR: &str = "recovery";

/// Essential application state that is persisted for crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppStateSnapshot {
    /// The active session ID.
    pub session_id: String,

    /// Number of messages exchanged so far.
    pub message_count: usize,

    /// Last tool results (tool name -> truncated output), limited to most recent.
    pub last_tool_results: Vec<ToolResultEntry>,

    /// Timestamp (ms since epoch) when the snapshot was taken.
    pub snapshot_timestamp_ms: u64,

    /// Whether the session ended cleanly.
    pub completed: bool,

    /// Project directory associated with the session.
    pub project_dir: String,

    /// Cumulative cost in USD.
    pub cost_usd: f64,
}

/// A single tool result entry stored in the snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResultEntry {
    pub tool_name: String,
    pub call_id: String,
    /// Truncated output (first N chars).
    pub output_preview: String,
    pub success: bool,
}

impl AppStateSnapshot {
    /// Create a new snapshot with the given session ID.
    pub fn new(session_id: impl Into<String>, project_dir: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            message_count: 0,
            last_tool_results: Vec::new(),
            snapshot_timestamp_ms: crate::event_bus::now_ms(),
            completed: false,
            project_dir: project_dir.into(),
            cost_usd: 0.0,
        }
    }

    /// Record a tool result. Keeps at most `max_entries` most recent results.
    pub fn record_tool_result(&mut self, entry: ToolResultEntry, max_entries: usize) {
        self.last_tool_results.push(entry);
        if self.last_tool_results.len() > max_entries {
            let excess = self.last_tool_results.len() - max_entries;
            self.last_tool_results.drain(..excess);
        }
    }

    /// Mark the session as completed (clean exit).
    pub fn mark_completed(&mut self) {
        self.completed = true;
        self.snapshot_timestamp_ms = crate::event_bus::now_ms();
    }
}

// ---------------------------------------------------------------------------
// SnapshotPersistence — read / write to disk
// ---------------------------------------------------------------------------

/// Manages reading and writing [`AppStateSnapshot`] to disk.
#[derive(Debug, Clone)]
pub struct SnapshotPersistence {
    snapshot_dir: PathBuf,
}

impl SnapshotPersistence {
    /// Create a persistence manager using the default snapshot directory.
    pub fn new() -> Self {
        let snapshot_dir = dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".opendev")
            .join("data")
            .join(SNAPSHOT_SUBDIR);

        Self { snapshot_dir }
    }

    /// Create a persistence manager using a custom directory (useful for tests).
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self {
        Self {
            snapshot_dir: dir.into(),
        }
    }

    /// Return the path where a session's snapshot would be stored.
    pub fn snapshot_path(&self, session_id: &str) -> PathBuf {
        self.snapshot_dir
            .join(format!("{session_id}_{SNAPSHOT_FILENAME}"))
    }

    /// Save a snapshot to disk atomically (write tmp then rename).
    pub fn save(&self, snapshot: &AppStateSnapshot) -> Result<(), String> {
        std::fs::create_dir_all(&self.snapshot_dir)
            .map_err(|e| format!("Failed to create snapshot dir: {e}"))?;

        let path = self.snapshot_path(&snapshot.session_id);
        let tmp_path = path.with_extension(format!("json.tmp.{}", uuid::Uuid::new_v4()));

        let json = serde_json::to_string_pretty(snapshot)
            .map_err(|e| format!("Failed to serialize snapshot: {e}"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true).mode(0o600);

            let mut file = opts
                .open(&tmp_path)
                .map_err(|e| format!("Failed to open snapshot tmp: {e}"))?;
            std::io::Write::write_all(&mut file, json.as_bytes())
                .map_err(|e| format!("Failed to write snapshot tmp: {e}"))?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&tmp_path, &json)
                .map_err(|e| format!("Failed to write snapshot tmp: {e}"))?;
        }

        std::fs::rename(&tmp_path, &path).map_err(|e| format!("Failed to rename snapshot: {e}"))?;

        debug!("Saved snapshot for session {}", snapshot.session_id);
        Ok(())
    }

    /// Load a snapshot for a specific session.
    pub fn load(&self, session_id: &str) -> Option<AppStateSnapshot> {
        let path = self.snapshot_path(session_id);
        self.load_from_path(&path)
    }

    /// Load a snapshot from a specific path.
    fn load_from_path(&self, path: &Path) -> Option<AppStateSnapshot> {
        let contents = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    /// Find all incomplete (non-completed) session snapshots.
    ///
    /// Returns snapshots where `completed == false`, sorted by timestamp
    /// (most recent first).
    pub fn find_incomplete_sessions(&self) -> Vec<AppStateSnapshot> {
        let mut snapshots = Vec::new();

        let entries = match std::fs::read_dir(&self.snapshot_dir) {
            Ok(e) => e,
            Err(_) => return snapshots,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json")
                && let Some(snapshot) = self.load_from_path(&path)
                && !snapshot.completed
            {
                snapshots.push(snapshot);
            }
        }

        snapshots.sort_by_key(|b| std::cmp::Reverse(b.snapshot_timestamp_ms));
        snapshots
    }

    /// Remove the snapshot file for a session (e.g., after clean exit or recovery).
    pub fn remove(&self, session_id: &str) -> bool {
        let path = self.snapshot_path(session_id);
        match std::fs::remove_file(&path) {
            Ok(()) => {
                debug!("Removed snapshot for session {session_id}");
                true
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    warn!("Failed to remove snapshot {}: {e}", path.display());
                }
                false
            }
        }
    }

    /// Remove snapshots older than `max_age`.
    pub fn cleanup_old(&self, max_age: Duration) -> usize {
        let cutoff_ms = crate::event_bus::now_ms().saturating_sub(max_age.as_millis() as u64);
        let mut removed = 0;

        let entries = match std::fs::read_dir(&self.snapshot_dir) {
            Ok(e) => e,
            Err(_) => return 0,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json")
                && let Some(snapshot) = self.load_from_path(&path)
                && snapshot.snapshot_timestamp_ms < cutoff_ms
                && std::fs::remove_file(&path).is_ok()
            {
                removed += 1;
            }
        }

        removed
    }
}

impl Default for SnapshotPersistence {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "state_snapshot_tests.rs"]
mod tests;
