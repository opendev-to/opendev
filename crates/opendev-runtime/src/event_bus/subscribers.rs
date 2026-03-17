//! Subscriber types for the event bus.
//!
//! [`TopicSubscriber`] provides topic-based filtering, while
//! [`FilteredSubscriber`] provides legacy string-based filtering.

use std::collections::HashSet;

use tokio::sync::broadcast;
use tracing::debug;

use super::{Event, EventBus, EventTopic, RuntimeEvent};

// ---------------------------------------------------------------------------
// TopicSubscriber -- topic-based filtering (#94)
// ---------------------------------------------------------------------------

/// A subscriber that only receives events matching its declared topics.
pub struct TopicSubscriber {
    receiver: broadcast::Receiver<RuntimeEvent>,
    topics: HashSet<EventTopic>,
}

impl TopicSubscriber {
    /// Create a new topic subscriber.
    pub(super) fn new(
        receiver: broadcast::Receiver<RuntimeEvent>,
        topics: HashSet<EventTopic>,
    ) -> Self {
        Self { receiver, topics }
    }

    /// Receive the next event matching the subscriber's topics.
    pub async fn recv(&mut self) -> Option<RuntimeEvent> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    if self.topics.contains(&event.topic()) {
                        return Some(event);
                    }
                    // Not interested -- skip.
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!("TopicSubscriber lagged, missed {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    /// Return the set of topics this subscriber is interested in.
    pub fn topics(&self) -> &HashSet<EventTopic> {
        &self.topics
    }
}

// ---------------------------------------------------------------------------
// FilteredSubscriber -- legacy string-based filtering (backward compat)
// ---------------------------------------------------------------------------

/// Filtered event subscriber -- only receives events matching a filter.
///
/// Works with the legacy `event_type` string inside `RuntimeEvent::Custom`.
pub struct FilteredSubscriber {
    receiver: broadcast::Receiver<RuntimeEvent>,
    event_types: Option<Vec<String>>,
}

impl FilteredSubscriber {
    /// Create a filtered subscriber.
    pub fn new(bus: &EventBus, event_types: Option<Vec<String>>) -> Self {
        Self {
            receiver: bus.subscribe(),
            event_types,
        }
    }

    /// Receive the next matching event (returns a legacy `Event`).
    pub async fn recv(&mut self) -> Option<Event> {
        loop {
            match self.receiver.recv().await {
                Ok(runtime_event) => {
                    // Convert RuntimeEvent to legacy Event for compat.
                    let legacy = match &runtime_event {
                        RuntimeEvent::Custom {
                            event_type,
                            source,
                            data,
                            timestamp_ms,
                        } => Event {
                            event_type: event_type.clone(),
                            source: source.clone(),
                            data: data.clone(),
                            timestamp_ms: *timestamp_ms,
                        },
                        other => Event {
                            event_type: format!("{:?}", other.topic()),
                            source: String::new(),
                            data: serde_json::to_value(other).unwrap_or(serde_json::Value::Null),
                            timestamp_ms: other.timestamp_ms(),
                        },
                    };

                    if let Some(ref types) = self.event_types
                        && !types.iter().any(|t| t == &legacy.event_type)
                    {
                        continue;
                    }
                    return Some(legacy);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!("Subscriber lagged, missed {n} events");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}
