use std::collections::HashSet;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use chrono::Utc;
use opendev_history::event_store::{EventEnvelope, EventStore, SessionEvent};

use crate::event_bus::{EventBus, EventTopic, RuntimeEvent, create_event_bus_bridge, now_ms};

#[test]
fn test_session_mutation_event_topic() {
    let ev = RuntimeEvent::SessionMutation {
        session_id: "s1".into(),
        event_type: "MessageAdded".into(),
        seq: 1,
        timestamp_ms: 12345,
    };
    assert_eq!(ev.topic(), EventTopic::Session);
}

#[test]
fn test_session_mutation_timestamp() {
    let ev = RuntimeEvent::SessionMutation {
        session_id: "s1".into(),
        event_type: "TitleChanged".into(),
        seq: 3,
        timestamp_ms: 99999,
    };
    assert_eq!(ev.timestamp_ms(), 99999);
}

#[test]
fn test_session_mutation_serialization() {
    let ev = RuntimeEvent::SessionMutation {
        session_id: "s1".into(),
        event_type: "MessageAdded".into(),
        seq: 7,
        timestamp_ms: 55555,
    };
    let json = serde_json::to_string(&ev).unwrap();
    let deserialized: RuntimeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.topic(), EventTopic::Session);
    assert_eq!(deserialized.timestamp_ms(), 55555);
    if let RuntimeEvent::SessionMutation {
        session_id,
        event_type,
        seq,
        ..
    } = deserialized
    {
        assert_eq!(session_id, "s1");
        assert_eq!(event_type, "MessageAdded");
        assert_eq!(seq, 7);
    } else {
        panic!("expected SessionMutation variant");
    }
}

#[tokio::test]
async fn test_session_mutation_received_by_session_subscriber() {
    let bus = EventBus::new();
    let mut sub = bus.subscribe_topics(HashSet::from([EventTopic::Session]));

    bus.publish(RuntimeEvent::SessionMutation {
        session_id: "s1".into(),
        event_type: "MessageAdded".into(),
        seq: 1,
        timestamp_ms: now_ms(),
    });

    let received = sub.recv().await.unwrap();
    assert!(matches!(received, RuntimeEvent::SessionMutation { .. }));
}

#[test]
fn test_post_append_callback_invoked() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let count_clone = call_count.clone();

    let callback = Arc::new(move |_agg: &str, envs: &[EventEnvelope]| {
        count_clone.fetch_add(envs.len(), Ordering::SeqCst);
    });

    let dir = tempfile::tempdir().unwrap();
    let store = EventStore::new(dir.path().to_path_buf()).with_post_append(callback);

    let events = vec![
        SessionEvent::TitleChanged { title: "Test1".into() },
        SessionEvent::TitleChanged { title: "Test2".into() },
    ];

    store.append("session-1", events).unwrap();

    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_event_bus_bridge_publishes_mutations() {
    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    let bridge = create_event_bus_bridge(bus.clone());

    let event = SessionEvent::MessageAdded {
        role: "user".into(),
        content: "hello".into(),
        timestamp: Utc::now(),
        tool_calls: vec![],
        tokens: Some(10),
        thinking_trace: None,
        reasoning_content: None,
    };
    let envelopes = vec![EventEnvelope::new("sess-42", 1, &event)];

    bridge("sess-42", &envelopes);

    let received = rx.recv().await.unwrap();
    if let RuntimeEvent::SessionMutation {
        session_id,
        event_type,
        seq,
        ..
    } = received
    {
        assert_eq!(session_id, "sess-42");
        assert_eq!(event_type, "MessageAdded");
        assert_eq!(seq, 1);
    } else {
        panic!("expected SessionMutation");
    }
}

#[tokio::test]
async fn test_event_store_with_bridge_end_to_end() {
    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    let bridge = create_event_bus_bridge(bus.clone());

    let dir = tempfile::tempdir().unwrap();
    let store = EventStore::new(dir.path().to_path_buf()).with_post_append(bridge);

    let events = vec![SessionEvent::TitleChanged {
        title: "New Title".into(),
    }];

    store.append("sess-1", events).unwrap();

    let received = rx.recv().await.unwrap();
    if let RuntimeEvent::SessionMutation {
        session_id,
        event_type,
        seq,
        ..
    } = received
    {
        assert_eq!(session_id, "sess-1");
        assert_eq!(event_type, "TitleChanged");
        assert_eq!(seq, 1);
    } else {
        panic!("expected SessionMutation");
    }
}
