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
mod tests;
