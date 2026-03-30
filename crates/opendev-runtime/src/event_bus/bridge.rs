//! Bridge between the [`EventStore`] post-append callback and the [`EventBus`].
//!
//! The [`create_event_bus_bridge`] function returns a [`PostAppendCallback`]
//! that publishes a [`RuntimeEvent::SessionMutation`] for every persisted
//! envelope, enabling real-time notification to WebSocket clients and other
//! event bus subscribers.

use std::sync::Arc;

use opendev_history::event_store::{EventEnvelope, PostAppendCallback};

use super::{EventBus, RuntimeEvent};

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;

/// Create a [`PostAppendCallback`] that publishes [`RuntimeEvent::SessionMutation`]
/// to the given [`EventBus`] for each persisted event.
pub fn create_event_bus_bridge(event_bus: EventBus) -> PostAppendCallback {
    Arc::new(move |aggregate_id: &str, envelopes: &[EventEnvelope]| {
        for envelope in envelopes {
            event_bus.publish(RuntimeEvent::SessionMutation {
                session_id: aggregate_id.to_string(),
                event_type: envelope.event_type.clone(),
                seq: envelope.seq,
                timestamp_ms: envelope.timestamp.timestamp_millis() as u64,
            });
        }
    })
}
