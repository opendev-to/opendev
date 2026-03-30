//! Event sourcing primitives for session history.
//!
//! Defines [`SessionEvent`] (the domain events), [`EventEnvelope`] (the
//! persistence wrapper), and the [`ValidateTransition`] implementation that
//! guards session state transitions.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use opendev_models::file_change::FileChange;
use opendev_models::message::ToolCall;
use opendev_models::session::Session;
use opendev_models::transition::{TransitionError, ValidateTransition};

use crate::file_locks::FileLock;

// ---------------------------------------------------------------------------
// SessionEvent
// ---------------------------------------------------------------------------

/// Domain events that can occur within a session aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionEvent {
    SessionCreated {
        id: String,
        working_directory: Option<String>,
        channel: String,
        title: Option<String>,
        parent_id: Option<String>,
        metadata: HashMap<String, Value>,
    },
    MessageAdded {
        role: String,
        content: String,
        #[serde(with = "opendev_models::datetime_compat")]
        timestamp: DateTime<Utc>,
        tool_calls: Vec<ToolCall>,
        tokens: Option<u64>,
        thinking_trace: Option<String>,
        reasoning_content: Option<String>,
    },
    MessageEdited {
        seq: usize,
        content: String,
    },
    TitleChanged {
        title: String,
    },
    SessionArchived {
        #[serde(with = "opendev_models::datetime_compat")]
        time_archived: DateTime<Utc>,
    },
    SessionUnarchived,
    FileChangeRecorded {
        file_change: FileChange,
    },
    MetadataUpdated {
        key: String,
        value: Value,
    },
    SessionForked {
        source_session_id: String,
        fork_point: Option<usize>,
    },
}

impl SessionEvent {
    /// Returns the variant name as a static string, matching the enum discriminant.
    pub fn event_type(&self) -> &'static str {
        match self {
            SessionEvent::SessionCreated { .. } => "SessionCreated",
            SessionEvent::MessageAdded { .. } => "MessageAdded",
            SessionEvent::MessageEdited { .. } => "MessageEdited",
            SessionEvent::TitleChanged { .. } => "TitleChanged",
            SessionEvent::SessionArchived { .. } => "SessionArchived",
            SessionEvent::SessionUnarchived => "SessionUnarchived",
            SessionEvent::FileChangeRecorded { .. } => "FileChangeRecorded",
            SessionEvent::MetadataUpdated { .. } => "MetadataUpdated",
            SessionEvent::SessionForked { .. } => "SessionForked",
        }
    }
}

// ---------------------------------------------------------------------------
// EventEnvelope
// ---------------------------------------------------------------------------

/// Persistence wrapper that pairs a domain event with routing/ordering metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// Unique event id (UUID v4).
    pub id: String,
    /// The aggregate (session) this event belongs to.
    pub aggregate_id: String,
    /// Monotonically increasing sequence number within the aggregate.
    pub seq: u64,
    /// Discriminant name, e.g. `"SessionCreated"`.
    pub event_type: String,
    /// Serialized [`SessionEvent`] payload.
    pub data: Value,
    /// Wall-clock time the event was created.
    #[serde(with = "opendev_models::datetime_compat")]
    pub timestamp: DateTime<Utc>,
}

impl EventEnvelope {
    /// Construct an envelope from a domain event.
    pub fn new(aggregate_id: impl Into<String>, seq: u64, event: &SessionEvent) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            aggregate_id: aggregate_id.into(),
            seq,
            event_type: event.event_type().to_string(),
            data: serde_json::to_value(event).expect("SessionEvent must be serializable"),
            timestamp: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// PostAppendCallback + EventStore
// ---------------------------------------------------------------------------

/// Callback invoked after events are persisted to disk.
/// Receives the aggregate_id and the list of persisted envelopes.
pub type PostAppendCallback = Arc<dyn Fn(&str, &[EventEnvelope]) + Send + Sync>;

/// JSONL-file-backed event store.
///
/// Each aggregate (session) gets its own event log file at
/// `{sessions_dir}/{aggregate_id}.events.jsonl`. Events are append-only
/// JSONL lines, each containing a serialized `EventEnvelope`.
///
/// The optional [`PostAppendCallback`] is invoked after events are
/// successfully written, enabling integration with an event bus.
pub struct EventStore {
    /// Base directory where event log files are stored.
    sessions_dir: PathBuf,
    /// Optional callback invoked after a successful append.
    post_append: Option<PostAppendCallback>,
}

/// Default timeout for file lock acquisition.
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

impl EventStore {
    /// Create a new event store rooted at `sessions_dir`.
    pub fn new(sessions_dir: PathBuf) -> Self {
        Self {
            sessions_dir,
            post_append: None,
        }
    }

    /// Builder method: attach a callback invoked after each successful append.
    pub fn with_post_append(mut self, callback: PostAppendCallback) -> Self {
        self.post_append = Some(callback);
        self
    }

    /// Return the base directory for session event files.
    pub fn sessions_dir(&self) -> &std::path::Path {
        &self.sessions_dir
    }

    /// Returns the path to the event log file for a given aggregate.
    pub fn event_log_path(&self, aggregate_id: &str) -> PathBuf {
        self.sessions_dir
            .join(format!("{}.events.jsonl", aggregate_id))
    }

    /// Append events to the aggregate's event log. Returns the created envelopes.
    ///
    /// Acquires an exclusive file lock, reads the current max sequence number,
    /// then appends each event as a JSON line with an incrementing seq.
    /// Invokes the post-append callback after successful persistence.
    pub fn append(
        &self,
        aggregate_id: &str,
        events: Vec<SessionEvent>,
    ) -> Result<Vec<EventEnvelope>, String> {
        if events.is_empty() {
            return Ok(vec![]);
        }

        let path = self.event_log_path(aggregate_id);
        let _lock =
            FileLock::acquire(&path, LOCK_TIMEOUT).map_err(|e| format!("lock failed: {e}"))?;

        // Read current max seq from the last line of the file.
        let mut last_seq = self.read_last_seq(&path);

        // Open file in append mode (create if needed).
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("open failed: {e}"))?;

        let mut envelopes = Vec::with_capacity(events.len());
        for event in &events {
            last_seq += 1;
            let envelope = EventEnvelope::new(aggregate_id, last_seq, event);
            let line =
                serde_json::to_string(&envelope).map_err(|e| format!("serialize failed: {e}"))?;
            writeln!(file, "{}", line).map_err(|e| format!("write failed: {e}"))?;
            envelopes.push(envelope);
        }

        file.flush().map_err(|e| format!("flush failed: {e}"))?;
        // _lock drops here, releasing the file lock.

        // Notify listeners after successful persistence.
        if let Some(cb) = &self.post_append {
            cb(aggregate_id, &envelopes);
        }

        Ok(envelopes)
    }

    /// Load all events for the given aggregate, sorted by seq.
    pub fn load(&self, aggregate_id: &str) -> Result<Vec<EventEnvelope>, String> {
        let path = self.event_log_path(aggregate_id);
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(format!("open failed: {e}")),
        };
        let reader = std::io::BufReader::new(file);

        let mut envelopes = Vec::new();
        for (i, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| format!("read line {} failed: {e}", i + 1))?;
            if line.trim().is_empty() {
                continue;
            }
            let envelope: EventEnvelope = serde_json::from_str(&line)
                .map_err(|e| format!("parse line {} failed: {e}", i + 1))?;
            envelopes.push(envelope);
        }

        envelopes.sort_by_key(|e| e.seq);
        Ok(envelopes)
    }

    /// Load events with seq strictly greater than `after_seq`.
    pub fn load_since(
        &self,
        aggregate_id: &str,
        after_seq: u64,
    ) -> Result<Vec<EventEnvelope>, String> {
        let mut envelopes = self.load(aggregate_id)?;
        envelopes.retain(|e| e.seq > after_seq);
        Ok(envelopes)
    }

    /// Return the highest sequence number for the aggregate, or 0 if none.
    pub fn latest_seq(&self, aggregate_id: &str) -> Result<u64, String> {
        let path = self.event_log_path(aggregate_id);
        Ok(self.read_last_seq(&path))
    }

    /// Check whether the aggregate has any persisted events.
    pub fn has_events(&self, aggregate_id: &str) -> bool {
        let path = self.event_log_path(aggregate_id);
        match std::fs::metadata(&path) {
            Ok(meta) => meta.len() > 0,
            Err(_) => false,
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Read the last line of a JSONL file and extract its seq, or return 0.
    fn read_last_seq(&self, path: &std::path::Path) -> u64 {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => return 0,
        };

        // Walk backwards to find the last non-empty line.
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines().rev() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(envelope) = serde_json::from_str::<EventEnvelope>(trimmed) {
                return envelope.seq;
            }
            break;
        }
        0
    }
}

// ---------------------------------------------------------------------------
// ValidateTransition impl
// ---------------------------------------------------------------------------

impl ValidateTransition<SessionEvent> for Session {
    fn validate_transition(&self, event: &SessionEvent) -> Result<(), TransitionError> {
        match event {
            SessionEvent::MessageAdded { .. } | SessionEvent::MessageEdited { .. } => {
                if self.is_archived() {
                    return Err(TransitionError::SessionArchived {
                        action: "add message to".to_string(),
                    });
                }
            }

            SessionEvent::TitleChanged { title } => {
                if self.is_archived() {
                    return Err(TransitionError::SessionArchived {
                        action: "change title of".to_string(),
                    });
                }
                if title.trim().is_empty() {
                    return Err(TransitionError::EmptyTitle);
                }
            }

            SessionEvent::FileChangeRecorded { .. } => {
                if self.is_archived() {
                    return Err(TransitionError::SessionArchived {
                        action: "record file change on".to_string(),
                    });
                }
            }

            SessionEvent::MetadataUpdated { .. } => {
                if self.is_archived() {
                    return Err(TransitionError::SessionArchived {
                        action: "update metadata on".to_string(),
                    });
                }
            }

            SessionEvent::SessionArchived { .. } => {
                if self.is_archived() {
                    return Err(TransitionError::AlreadyArchived);
                }
            }

            SessionEvent::SessionUnarchived => {
                if !self.is_archived() {
                    return Err(TransitionError::NotArchived);
                }
            }

            SessionEvent::SessionForked { fork_point, .. } => {
                if let Some(point) = fork_point
                    && *point > self.messages.len()
                {
                    return Err(TransitionError::ForkPointOutOfRange {
                        point: *point,
                        message_count: self.messages.len(),
                    });
                }
            }

            SessionEvent::SessionCreated { .. } => {}
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "event_store_tests.rs"]
mod tests;
