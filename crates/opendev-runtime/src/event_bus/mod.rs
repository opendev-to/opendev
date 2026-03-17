//! Typed event bus for decoupled inter-component communication.
//!
//! Components publish typed [`RuntimeEvent`] variants; subscribers receive
//! copies asynchronously. Supports topic-based filtering so each subscriber
//! only receives events it is interested in.
//!
//! Events are broadcast via `tokio::sync::broadcast`.

mod events;
mod subscribers;
mod utils;

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::debug;

// Re-export public API so that `crate::event_bus::X` paths remain unchanged.
pub use self::events::{Event, EventTopic, RuntimeEvent, now_ms};
pub use self::subscribers::{FilteredSubscriber, TopicSubscriber};
pub use self::utils::{group_events_by_type, group_runtime_events_by_topic};

/// Maximum number of events buffered per channel.
const DEFAULT_CAPACITY: usize = 256;

// ---------------------------------------------------------------------------
// EventBus -- typed publish / subscribe (#93 + #94)
// ---------------------------------------------------------------------------

/// Typed event bus for broadcasting [`RuntimeEvent`] instances.
#[derive(Clone)]
pub struct EventBus {
    inner: Arc<EventBusInner>,
}

struct EventBusInner {
    sender: broadcast::Sender<RuntimeEvent>,
    _capacity: usize,
}

impl EventBus {
    /// Create a new event bus with the default capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new event bus with a specific capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            inner: Arc::new(EventBusInner {
                sender,
                _capacity: capacity,
            }),
        }
    }

    /// Publish a typed event to all subscribers.
    pub fn publish(&self, event: RuntimeEvent) {
        let topic = event.topic();
        match self.inner.sender.send(event) {
            Ok(n) => debug!("Event {:?} sent to {} subscribers", topic, n),
            Err(_) => debug!("Event {:?} published with no subscribers", topic),
        }
    }

    /// Convenience: publish a legacy `Event` by converting it to `RuntimeEvent::Custom`.
    pub fn emit(&self, event_type: &str, source: &str, data: serde_json::Value) {
        let event = Event::new(event_type, source, data);
        self.publish(event.into_runtime_event());
    }

    /// Subscribe to *all* events (unfiltered).
    pub fn subscribe(&self) -> broadcast::Receiver<RuntimeEvent> {
        self.inner.sender.subscribe()
    }

    /// Subscribe with topic-based filtering (#94).
    ///
    /// The returned [`TopicSubscriber`] only yields events whose topic is in
    /// the given set.
    pub fn subscribe_topics(&self, topics: HashSet<EventTopic>) -> TopicSubscriber {
        TopicSubscriber::new(self.inner.sender.subscribe(), topics)
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.inner.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("subscribers", &self.subscriber_count())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = Event::new("test", "component", serde_json::json!({"key": "value"}));
        assert_eq!(event.event_type, "test");
        assert_eq!(event.source, "component");
        assert!(event.timestamp_ms > 0);
    }

    #[test]
    fn test_bus_creation() {
        let bus = EventBus::new();
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn test_publish_subscribe() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        bus.emit("test_event", "test", serde_json::json!({"count": 1}));

        let event = rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::Custom { .. }));
        if let RuntimeEvent::Custom {
            event_type, data, ..
        } = event
        {
            assert_eq!(event_type, "test_event");
            assert_eq!(data["count"], 1);
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        assert_eq!(bus.subscriber_count(), 2);

        bus.emit("event", "src", serde_json::json!(null));

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert!(matches!(e1, RuntimeEvent::Custom { .. }));
        assert!(matches!(e2, RuntimeEvent::Custom { .. }));
    }

    #[tokio::test]
    async fn test_filtered_subscriber() {
        let bus = EventBus::new();
        let mut sub = FilteredSubscriber::new(&bus, Some(vec!["wanted".to_string()]));

        bus.emit("unwanted", "src", serde_json::json!(null));
        bus.emit("wanted", "src", serde_json::json!({"ok": true}));

        let event = sub.recv().await.unwrap();
        assert_eq!(event.event_type, "wanted");
    }

    #[test]
    fn test_no_subscribers() {
        let bus = EventBus::new();
        // Should not panic
        bus.emit("event", "src", serde_json::json!(null));
    }

    #[test]
    fn test_group_events_by_type() {
        let events = vec![
            Event::new("a", "src", serde_json::json!(null)),
            Event::new("b", "src", serde_json::json!(null)),
            Event::new("a", "src", serde_json::json!(null)),
        ];
        let groups = group_events_by_type(&events);
        assert_eq!(groups["a"].len(), 2);
        assert_eq!(groups["b"].len(), 1);
    }

    #[test]
    fn test_bus_clone() {
        let bus1 = EventBus::new();
        let _rx = bus1.subscribe();
        let bus2 = bus1.clone();
        assert_eq!(bus2.subscriber_count(), 1);
    }

    #[test]
    fn test_debug_format() {
        let bus = EventBus::new();
        let debug_str = format!("{:?}", bus);
        assert!(debug_str.contains("EventBus"));
    }

    // -- New typed event tests --

    #[test]
    fn test_runtime_event_topic() {
        let ev = RuntimeEvent::ToolCallStart {
            tool_name: "bash".into(),
            call_id: "1".into(),
            timestamp_ms: now_ms(),
        };
        assert_eq!(ev.topic(), EventTopic::Tool);

        let ev = RuntimeEvent::LlmRequestStart {
            model: "gpt-4".into(),
            request_id: "r1".into(),
            timestamp_ms: now_ms(),
        };
        assert_eq!(ev.topic(), EventTopic::Llm);

        let ev = RuntimeEvent::AgentStart {
            agent_id: "a1".into(),
            task: "test".into(),
            timestamp_ms: now_ms(),
        };
        assert_eq!(ev.topic(), EventTopic::Agent);

        let ev = RuntimeEvent::SessionStart {
            session_id: "s1".into(),
            timestamp_ms: now_ms(),
        };
        assert_eq!(ev.topic(), EventTopic::Session);

        let ev = RuntimeEvent::TokenUsage {
            model: "gpt-4".into(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.01,
            timestamp_ms: now_ms(),
        };
        assert_eq!(ev.topic(), EventTopic::Cost);

        let ev = RuntimeEvent::ConfigReloaded {
            timestamp_ms: now_ms(),
        };
        assert_eq!(ev.topic(), EventTopic::System);
    }

    #[tokio::test]
    async fn test_typed_publish_subscribe() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        let ev = RuntimeEvent::ToolCallStart {
            tool_name: "bash".into(),
            call_id: "c1".into(),
            timestamp_ms: now_ms(),
        };
        bus.publish(ev);

        let received = rx.recv().await.unwrap();
        assert!(matches!(received, RuntimeEvent::ToolCallStart { .. }));
        if let RuntimeEvent::ToolCallStart { tool_name, .. } = received {
            assert_eq!(tool_name, "bash");
        }
    }

    #[tokio::test]
    async fn test_topic_subscriber_filters() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe_topics(HashSet::from([EventTopic::Tool]));

        // Publish an LLM event (should be filtered out)
        bus.publish(RuntimeEvent::LlmRequestStart {
            model: "gpt-4".into(),
            request_id: "r1".into(),
            timestamp_ms: now_ms(),
        });

        // Publish a Tool event (should be received)
        bus.publish(RuntimeEvent::ToolCallStart {
            tool_name: "bash".into(),
            call_id: "c1".into(),
            timestamp_ms: now_ms(),
        });

        let received = sub.recv().await.unwrap();
        assert_eq!(received.topic(), EventTopic::Tool);
    }

    #[tokio::test]
    async fn test_topic_subscriber_multiple_topics() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe_topics(HashSet::from([EventTopic::Tool, EventTopic::Session]));

        bus.publish(RuntimeEvent::LlmRequestStart {
            model: "m".into(),
            request_id: "r".into(),
            timestamp_ms: now_ms(),
        });
        bus.publish(RuntimeEvent::SessionStart {
            session_id: "s1".into(),
            timestamp_ms: now_ms(),
        });

        let received = sub.recv().await.unwrap();
        assert_eq!(received.topic(), EventTopic::Session);
    }

    #[test]
    fn test_legacy_event_into_runtime_event() {
        let legacy = Event::new("test", "comp", serde_json::json!(42));
        let rt = legacy.into_runtime_event();
        assert_eq!(rt.topic(), EventTopic::Custom);
        if let RuntimeEvent::Custom {
            event_type, data, ..
        } = rt
        {
            assert_eq!(event_type, "test");
            assert_eq!(data, serde_json::json!(42));
        } else {
            panic!("expected Custom variant");
        }
    }

    #[test]
    fn test_group_runtime_events_by_topic() {
        let events = vec![
            RuntimeEvent::ToolCallStart {
                tool_name: "a".into(),
                call_id: "1".into(),
                timestamp_ms: 0,
            },
            RuntimeEvent::LlmRequestStart {
                model: "m".into(),
                request_id: "r".into(),
                timestamp_ms: 0,
            },
            RuntimeEvent::ToolCallEnd {
                tool_name: "a".into(),
                call_id: "1".into(),
                duration_ms: 100,
                success: true,
                timestamp_ms: 0,
            },
        ];
        let groups = group_runtime_events_by_topic(&events);
        assert_eq!(groups[&EventTopic::Tool].len(), 2);
        assert_eq!(groups[&EventTopic::Llm].len(), 1);
    }

    #[test]
    fn test_topic_subscriber_topics_accessor() {
        let bus = EventBus::new();
        let topics = HashSet::from([EventTopic::Agent, EventTopic::Cost]);
        let sub = bus.subscribe_topics(topics.clone());
        assert_eq!(*sub.topics(), topics);
    }

    #[test]
    fn test_event_topic_serialization() {
        let topic = EventTopic::Tool;
        let json = serde_json::to_string(&topic).unwrap();
        let deserialized: EventTopic = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, topic);
    }

    #[test]
    fn test_runtime_event_serialization() {
        let event = RuntimeEvent::ToolCallStart {
            tool_name: "bash".into(),
            call_id: "c1".into(),
            timestamp_ms: 12345,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: RuntimeEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.topic(), EventTopic::Tool);
        assert_eq!(deserialized.timestamp_ms(), 12345);
    }

    #[test]
    fn test_now_ms_is_positive() {
        assert!(now_ms() > 0);
    }

    #[test]
    fn test_budget_exhausted_event_topic() {
        let ev = RuntimeEvent::BudgetExhausted {
            budget_usd: 1.0,
            total_cost_usd: 1.05,
            timestamp_ms: now_ms(),
        };
        assert_eq!(ev.topic(), EventTopic::Cost);
        assert!(ev.timestamp_ms() > 0);
    }

    #[test]
    fn test_budget_exhausted_serialization() {
        let event = RuntimeEvent::BudgetExhausted {
            budget_usd: 2.50,
            total_cost_usd: 2.75,
            timestamp_ms: 99999,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: RuntimeEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.topic(), EventTopic::Cost);
        assert_eq!(deserialized.timestamp_ms(), 99999);
        if let RuntimeEvent::BudgetExhausted {
            budget_usd,
            total_cost_usd,
            ..
        } = deserialized
        {
            assert!((budget_usd - 2.50).abs() < 1e-10);
            assert!((total_cost_usd - 2.75).abs() < 1e-10);
        } else {
            panic!("expected BudgetExhausted variant");
        }
    }

    #[tokio::test]
    async fn test_budget_exhausted_received_by_cost_subscriber() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe_topics(HashSet::from([EventTopic::Cost]));

        bus.publish(RuntimeEvent::BudgetExhausted {
            budget_usd: 1.0,
            total_cost_usd: 1.5,
            timestamp_ms: now_ms(),
        });

        let received = sub.recv().await.unwrap();
        assert!(matches!(received, RuntimeEvent::BudgetExhausted { .. }));
    }
}
