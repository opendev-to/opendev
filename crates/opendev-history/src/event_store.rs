//! Event sourcing primitives for session history.
//!
//! Defines [`SessionEvent`] (the domain events), [`EventEnvelope`] (the
//! persistence wrapper), and the [`ValidateTransition`] implementation that
//! guards session state transitions.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use opendev_models::file_change::FileChange;
use opendev_models::message::ToolCall;
use opendev_models::session::Session;
use opendev_models::transition::{TransitionError, ValidateTransition};

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
