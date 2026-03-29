//! Session status tracking for monitoring agent activity.
//!
//! Tracks per-session status (idle, busy, retry) and publishes changes
//! via the event bus. The TUI uses this to display retry countdowns
//! and activity indicators.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::event_bus::{EventBus, RuntimeEvent, now_ms};

/// The current status of a session.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionStatus {
    /// Session is idle, waiting for user input.
    #[default]
    #[serde(rename = "idle")]
    Idle,
    /// Session is actively processing a request.
    #[serde(rename = "busy")]
    Busy,
    /// Session is waiting to retry after an error.
    #[serde(rename = "retry")]
    Retry {
        /// Which retry attempt this is (1-based).
        attempt: u32,
        /// Human-readable reason for the retry (e.g. "Rate Limited").
        message: String,
        /// Unix timestamp in milliseconds when the next retry will occur.
        next_retry_ms: u64,
    },
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Busy => write!(f, "busy"),
            Self::Retry {
                attempt, message, ..
            } => write!(f, "retry (attempt {attempt}: {message})"),
        }
    }
}

/// Tracks the status of all active sessions.
///
/// Thread-safe via interior `Mutex`. Status changes are published
/// to an optional [`EventBus`].
pub struct SessionStatusTracker {
    state: Mutex<HashMap<String, SessionStatus>>,
    event_bus: Option<EventBus>,
}

impl SessionStatusTracker {
    /// Create a new tracker without event publishing.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
            event_bus: None,
        }
    }

    /// Create a new tracker that publishes status changes to the event bus.
    pub fn with_event_bus(event_bus: EventBus) -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
            event_bus: Some(event_bus),
        }
    }

    /// Get the current status of a session.
    ///
    /// Returns `SessionStatus::Idle` if the session has no tracked status.
    pub fn get(&self, session_id: &str) -> SessionStatus {
        self.state
            .lock()
            .expect("SessionStatusTracker lock poisoned")
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Set the status of a session.
    ///
    /// Setting `Idle` removes the session from tracking (it's the default).
    /// Publishes a `SessionStatusChanged` event if an event bus is configured.
    pub fn set(&self, session_id: impl Into<String>, status: SessionStatus) {
        let session_id = session_id.into();
        let mut state = self
            .state
            .lock()
            .expect("SessionStatusTracker lock poisoned");

        match &status {
            SessionStatus::Idle => {
                state.remove(&session_id);
            }
            _ => {
                state.insert(session_id.clone(), status.clone());
            }
        }

        // Publish event
        if let Some(bus) = &self.event_bus {
            bus.publish(RuntimeEvent::SessionStatusChanged {
                session_id,
                status,
                timestamp_ms: now_ms(),
            });
        }
    }

    /// Mark a session as busy.
    pub fn set_busy(&self, session_id: impl Into<String>) {
        self.set(session_id, SessionStatus::Busy);
    }

    /// Mark a session as idle.
    pub fn set_idle(&self, session_id: impl Into<String>) {
        self.set(session_id, SessionStatus::Idle);
    }

    /// Mark a session as retrying.
    pub fn set_retry(
        &self,
        session_id: impl Into<String>,
        attempt: u32,
        message: impl Into<String>,
        next_retry_ms: u64,
    ) {
        self.set(
            session_id,
            SessionStatus::Retry {
                attempt,
                message: message.into(),
                next_retry_ms,
            },
        );
    }

    /// Get all tracked sessions and their statuses.
    pub fn list(&self) -> HashMap<String, SessionStatus> {
        self.state
            .lock()
            .expect("SessionStatusTracker lock poisoned")
            .clone()
    }

    /// Get the number of tracked (non-idle) sessions.
    pub fn active_count(&self) -> usize {
        self.state
            .lock()
            .expect("SessionStatusTracker lock poisoned")
            .len()
    }

    /// Check if any session is currently retrying.
    pub fn has_retrying(&self) -> bool {
        self.state
            .lock()
            .expect("SessionStatusTracker lock poisoned")
            .values()
            .any(|s| matches!(s, SessionStatus::Retry { .. }))
    }
}

impl Default for SessionStatusTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SessionStatusTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.active_count();
        f.debug_struct("SessionStatusTracker")
            .field("active_sessions", &count)
            .finish()
    }
}

#[cfg(test)]
#[path = "session_status_tests.rs"]
mod tests;
