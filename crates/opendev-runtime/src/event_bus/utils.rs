//! Utility functions for grouping and analyzing events.

use std::collections::HashMap;

use super::{Event, EventTopic, RuntimeEvent};

/// Collect events into a map grouped by event type (useful for metrics).
pub fn group_events_by_type(events: &[Event]) -> HashMap<String, Vec<&Event>> {
    let mut groups: HashMap<String, Vec<&Event>> = HashMap::new();
    for event in events {
        groups
            .entry(event.event_type.clone())
            .or_default()
            .push(event);
    }
    groups
}

/// Group [`RuntimeEvent`]s by their [`EventTopic`].
pub fn group_runtime_events_by_topic(
    events: &[RuntimeEvent],
) -> HashMap<EventTopic, Vec<&RuntimeEvent>> {
    let mut groups: HashMap<EventTopic, Vec<&RuntimeEvent>> = HashMap::new();
    for event in events {
        groups.entry(event.topic()).or_default().push(event);
    }
    groups
}
