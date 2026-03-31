//! Session projector — replays event sequences into Session state.

use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use opendev_models::message::{ChatMessage, Role};
use opendev_models::session::Session;

use crate::event_store::{EventEnvelope, SessionEvent};

/// Replays a sequence of events to reconstruct a Session.
pub struct SessionProjector;

impl SessionProjector {
    /// Reconstruct a Session from a sequence of events.
    ///
    /// The first effective event MUST be `SessionCreated`. Returns error if
    /// events are empty or don't start with SessionCreated.
    ///
    /// Tombstone events are respected: any event with seq <= the latest
    /// tombstone's `undo_to_seq` is excluded from replay.
    pub fn project_from_events(events: &[EventEnvelope]) -> Result<Session, String> {
        let effective = crate::event_store::EventStore::effective_events(events);
        Self::project_from_effective(&effective)
    }

    /// Reconstruct a Session from events up to (and including) `target_seq`.
    ///
    /// This gives a point-in-time view of the session state. Only events
    /// with seq <= `target_seq` are considered (tombstone filtering still
    /// applies within that range).
    pub fn replay_to(events: &[EventEnvelope], target_seq: u64) -> Result<Session, String> {
        let truncated: Vec<EventEnvelope> = events
            .iter()
            .filter(|e| e.seq <= target_seq)
            .cloned()
            .collect();
        let effective = crate::event_store::EventStore::effective_events(&truncated);
        Self::project_from_effective(&effective)
    }

    /// Internal: project from a pre-filtered list of effective event references.
    fn project_from_effective(events: &[&EventEnvelope]) -> Result<Session, String> {
        let first = events
            .first()
            .ok_or_else(|| "Cannot project from empty events".to_string())?;

        let first_event: SessionEvent = serde_json::from_value(first.data.clone())
            .map_err(|e| format!("Failed to deserialize first event: {e}"))?;

        let SessionEvent::SessionCreated {
            id,
            working_directory,
            channel,
            title,
            parent_id,
            metadata,
        } = first_event
        else {
            return Err("First event must be SessionCreated".to_string());
        };

        let mut session = Session::new();
        session.id = id;
        session.created_at = first.timestamp;
        session.updated_at = first.timestamp;
        session.working_directory = working_directory;
        session.channel = channel;
        session.parent_id = parent_id;
        session.metadata = metadata;

        if let Some(t) = title {
            session
                .metadata
                .insert("title".to_string(), serde_json::Value::String(t));
        }

        for envelope in &events[1..] {
            Self::apply_event(&mut session, envelope)?;
        }

        Ok(session)
    }

    /// Apply a single event to an existing Session.
    pub fn apply_event(session: &mut Session, envelope: &EventEnvelope) -> Result<(), String> {
        let event: SessionEvent = serde_json::from_value(envelope.data.clone())
            .map_err(|e| format!("Failed to deserialize event: {e}"))?;
        Self::apply_session_event(session, &event, envelope.timestamp)
    }

    /// Apply a domain event directly (avoids serialize/deserialize round-trip).
    pub fn apply_session_event(
        session: &mut Session,
        event: &SessionEvent,
        timestamp: DateTime<Utc>,
    ) -> Result<(), String> {
        match event.clone() {
            SessionEvent::SessionCreated { .. } => {
                return Err("SessionCreated can only be the first event".to_string());
            }
            SessionEvent::MessageAdded {
                role,
                content,
                timestamp,
                tool_calls,
                tokens,
                thinking_trace,
                reasoning_content,
            } => {
                let parsed_role =
                    Role::from_str(&role).map_err(|_| format!("Unknown role: {role}"))?;

                let msg = ChatMessage {
                    role: parsed_role,
                    content,
                    timestamp,
                    metadata: HashMap::new(),
                    tool_calls,
                    tokens,
                    thinking_trace,
                    reasoning_content,
                    token_usage: None,
                    provenance: None,
                };
                session.messages.push(msg);
                session.updated_at = timestamp;
            }
            SessionEvent::MessageEdited { seq, content } => {
                let msg = session
                    .messages
                    .get_mut(seq)
                    .ok_or_else(|| format!("Message index {seq} out of range"))?;
                msg.content = content;
                session.updated_at = Utc::now();
            }
            SessionEvent::TitleChanged { title } => {
                session
                    .metadata
                    .insert("title".to_string(), serde_json::Value::String(title));
                session.updated_at = timestamp;
            }
            SessionEvent::SessionArchived { time_archived } => {
                session.time_archived = Some(time_archived);
            }
            SessionEvent::SessionUnarchived => {
                session.time_archived = None;
            }
            SessionEvent::FileChangeRecorded { file_change } => {
                session.file_changes.push(file_change);
                session.updated_at = timestamp;
            }
            SessionEvent::MetadataUpdated { key, value } => {
                session.metadata.insert(key, value);
                session.updated_at = timestamp;
            }
            SessionEvent::SessionForked {
                source_session_id,
                fork_point,
            } => {
                session.parent_id = Some(source_session_id);
                if let Some(point) = fork_point {
                    session.messages.truncate(point);
                }
                session.updated_at = timestamp;
            }
            SessionEvent::Tombstone { .. } => {
                // Tombstone events are handled at the filtering layer
                // (effective_events / project_from_events). If one reaches
                // apply_event, it's a no-op.
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "projector_tests.rs"]
mod tests;
