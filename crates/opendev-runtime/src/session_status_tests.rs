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
