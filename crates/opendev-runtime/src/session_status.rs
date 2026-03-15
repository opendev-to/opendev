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
mod tests {
    use super::*;

    #[test]
    fn test_default_is_idle() {
        let tracker = SessionStatusTracker::new();
        assert_eq!(tracker.get("nonexistent"), SessionStatus::Idle);
    }

    #[test]
    fn test_set_busy() {
        let tracker = SessionStatusTracker::new();
        tracker.set_busy("sess-1");
        assert_eq!(tracker.get("sess-1"), SessionStatus::Busy);
        assert_eq!(tracker.active_count(), 1);
    }

    #[test]
    fn test_set_idle_removes() {
        let tracker = SessionStatusTracker::new();
        tracker.set_busy("sess-1");
        assert_eq!(tracker.active_count(), 1);

        tracker.set_idle("sess-1");
        assert_eq!(tracker.get("sess-1"), SessionStatus::Idle);
        assert_eq!(tracker.active_count(), 0);
    }

    #[test]
    fn test_set_retry() {
        let tracker = SessionStatusTracker::new();
        tracker.set_retry("sess-1", 2, "Rate Limited", 1700000005000);
        match tracker.get("sess-1") {
            SessionStatus::Retry {
                attempt,
                message,
                next_retry_ms,
            } => {
                assert_eq!(attempt, 2);
                assert_eq!(message, "Rate Limited");
                assert_eq!(next_retry_ms, 1700000005000);
            }
            other => panic!("expected Retry, got {other:?}"),
        }
    }

    #[test]
    fn test_has_retrying() {
        let tracker = SessionStatusTracker::new();
        assert!(!tracker.has_retrying());

        tracker.set_busy("sess-1");
        assert!(!tracker.has_retrying());

        tracker.set_retry("sess-2", 1, "Overloaded", 0);
        assert!(tracker.has_retrying());

        tracker.set_idle("sess-2");
        assert!(!tracker.has_retrying());
    }

    #[test]
    fn test_list() {
        let tracker = SessionStatusTracker::new();
        tracker.set_busy("a");
        tracker.set_busy("b");
        tracker.set_retry("c", 1, "err", 0);

        let list = tracker.list();
        assert_eq!(list.len(), 3);
        assert_eq!(list.get("a"), Some(&SessionStatus::Busy));
        assert!(matches!(list.get("c"), Some(SessionStatus::Retry { .. })));
    }

    #[test]
    fn test_multiple_sessions() {
        let tracker = SessionStatusTracker::new();
        tracker.set_busy("sess-1");
        tracker.set_retry("sess-2", 1, "err", 0);
        tracker.set_busy("sess-3");

        assert_eq!(tracker.active_count(), 3);
        assert_eq!(tracker.get("sess-1"), SessionStatus::Busy);
        assert!(matches!(tracker.get("sess-2"), SessionStatus::Retry { .. }));

        // Transition sess-1 from busy to retry
        tracker.set_retry("sess-1", 1, "Too Many Requests", 99999);
        assert!(matches!(
            tracker.get("sess-1"),
            SessionStatus::Retry { attempt: 1, .. }
        ));
    }

    #[test]
    fn test_status_display() {
        assert_eq!(SessionStatus::Idle.to_string(), "idle");
        assert_eq!(SessionStatus::Busy.to_string(), "busy");
        assert_eq!(
            SessionStatus::Retry {
                attempt: 3,
                message: "Rate Limited".to_string(),
                next_retry_ms: 0,
            }
            .to_string(),
            "retry (attempt 3: Rate Limited)"
        );
    }

    #[test]
    fn test_status_serde() {
        let idle_json = serde_json::to_string(&SessionStatus::Idle).unwrap();
        assert!(idle_json.contains("\"type\":\"idle\""));

        let retry = SessionStatus::Retry {
            attempt: 2,
            message: "Overloaded".to_string(),
            next_retry_ms: 1234567890000,
        };
        let json = serde_json::to_string(&retry).unwrap();
        let deserialized: SessionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, retry);
    }

    #[test]
    fn test_debug_format() {
        let tracker = SessionStatusTracker::new();
        let debug = format!("{tracker:?}");
        assert!(debug.contains("SessionStatusTracker"));
        assert!(debug.contains("active_sessions"));
    }

    #[tokio::test]
    async fn test_event_bus_integration() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let tracker = SessionStatusTracker::with_event_bus(bus);

        tracker.set_busy("sess-1");
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::SessionStatusChanged { .. }));
        if let RuntimeEvent::SessionStatusChanged {
            session_id, status, ..
        } = event
        {
            assert_eq!(session_id, "sess-1");
            assert_eq!(status, SessionStatus::Busy);
        }
    }

    #[tokio::test]
    async fn test_event_bus_idle_publishes() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let tracker = SessionStatusTracker::with_event_bus(bus);

        tracker.set_busy("sess-1");
        let _ = rx.recv().await.unwrap(); // consume busy event

        tracker.set_idle("sess-1");
        let event = rx.recv().await.unwrap();
        if let RuntimeEvent::SessionStatusChanged { status, .. } = event {
            assert_eq!(status, SessionStatus::Idle);
        }
    }
}
