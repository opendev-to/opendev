//! Session manager: JSON read/write to ~/.opendev/sessions/.
//!
//! Handles session file I/O, including reading from both legacy JSON format
//! (all-in-one) and the newer JSON+JSONL split format.

mod operations;
pub mod titles;

use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use opendev_models::Session;
use opendev_models::message::ChatMessage;
use opendev_models::validator::{filter_and_repair_messages, validate_message};

use crate::event_store::{EventStore, SessionEvent};
use crate::index::SessionIndex;

pub use titles::{generate_title_from_messages, get_forked_title};

/// Session manager for persisting and loading sessions.
pub struct SessionManager {
    pub(super) session_dir: PathBuf,
    pub(super) index: SessionIndex,
    pub(super) current_session: Option<Session>,
    /// Optional event store for audit trail. When set, mutations also
    /// append SessionEvents to the JSONL event log.
    event_store: Option<EventStore>,
}

impl SessionManager {
    /// Create a new session manager.
    ///
    /// The `session_dir` is typically `~/.opendev/projects/{encoded-path}/`
    /// or `~/.opendev/sessions/` for the legacy global directory.
    pub fn new(session_dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&session_dir)?;
        let index = SessionIndex::new(session_dir.clone());
        Ok(Self {
            session_dir,
            index,
            current_session: None,
            event_store: None,
        })
    }

    /// Builder method: attach an event store for audit logging.
    pub fn with_event_store(mut self, store: EventStore) -> Self {
        self.event_store = Some(store);
        self
    }

    /// Return a reference to the event store, if configured.
    pub fn event_store(&self) -> Option<&EventStore> {
        self.event_store.as_ref()
    }

    /// Get the session directory.
    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    /// Write a marker file recording the original working directory.
    ///
    /// Used by cleanup routines to detect stale project entries
    /// whose original working directory has been deleted.
    pub fn write_project_marker(&self, working_dir: &Path) {
        let marker = self.session_dir.join("OPENDEV_PROJECT_PATH");
        let _ = std::fs::write(&marker, working_dir.to_string_lossy().as_bytes());
    }

    /// Get the current session (if any).
    pub fn current_session(&self) -> Option<&Session> {
        self.current_session.as_ref()
    }

    /// Get mutable reference to the current session.
    pub fn current_session_mut(&mut self) -> Option<&mut Session> {
        self.current_session.as_mut()
    }

    /// Set the current session.
    pub fn set_current_session(&mut self, session: Session) {
        self.current_session = Some(session);
    }

    /// Create a new session and set it as current.
    pub fn create_session(&mut self) -> &Session {
        let session = Session::new();
        info!("Created new session: {}", session.id);

        self.emit_event(
            &session.id,
            SessionEvent::SessionCreated {
                id: session.id.clone(),
                working_directory: session.working_directory.clone(),
                channel: session.channel.clone(),
                title: None,
                parent_id: None,
                metadata: session.metadata.clone(),
            },
        );

        self.current_session = Some(session);
        // SAFETY: we just set current_session to Some on the line above
        self.current_session
            .as_ref()
            .expect("current_session was just set to Some")
    }

    /// Add a validated message to the current session.
    ///
    /// Returns `true` if the message was accepted, `false` if rejected.
    pub fn add_message(&mut self, msg: ChatMessage) -> bool {
        let verdict = validate_message(&msg);
        if !verdict.is_valid {
            warn!("Rejected message: {}", verdict.reason);
            return false;
        }
        let Some(session) = &mut self.current_session else {
            return false;
        };

        // Build the event before pushing `msg` (which moves it), but only
        // if the event store is configured to avoid cloning on the hot path.
        let event = self
            .event_store
            .as_ref()
            .map(|_| SessionEvent::MessageAdded {
                role: msg.role.to_string(),
                content: msg.content.clone(),
                timestamp: msg.timestamp,
                tool_calls: msg.tool_calls.clone(),
                tokens: msg.tokens,
                thinking_trace: msg.thinking_trace.clone(),
                reasoning_content: msg.reasoning_content.clone(),
            });
        let session_id = session.id.clone();

        session.messages.push(msg);
        session.updated_at = chrono::Utc::now();

        if let Some(event) = event {
            self.emit_event(&session_id, event);
        }
        true
    }

    /// Save a session to disk.
    ///
    /// Writes session metadata to `{id}.json` and messages to `{id}.jsonl`.
    /// If the session has no title in its metadata, one is auto-generated
    /// from the first user message.
    /// Messages are validated and repaired before writing.
    pub fn save_session(&self, session: &Session) -> std::io::Result<()> {
        let json_path = self.session_dir.join(format!("{}.json", session.id));
        let jsonl_path = self.session_dir.join(format!("{}.jsonl", session.id));

        // Write metadata (session without messages for the JSON file)
        let mut session_for_json = session.clone();

        // Auto-generate title if not set
        if !session_for_json.metadata.contains_key("title")
            && let Some(title) = generate_title_from_messages(&session.messages)
        {
            session_for_json
                .metadata
                .insert("title".to_string(), serde_json::Value::String(title));
        }
        session_for_json.messages.clear();

        let json_content =
            serde_json::to_string_pretty(&session_for_json).map_err(std::io::Error::other)?;

        // Atomic write for metadata
        let tmp_json = self.session_dir.join(format!(".{}.json.tmp", session.id));
        std::fs::write(&tmp_json, &json_content)?;
        std::fs::rename(&tmp_json, &json_path)?;

        // Validate and repair messages before writing
        let mut valid_messages = session.messages.clone();
        let (dropped, repaired) = filter_and_repair_messages(&mut valid_messages);
        if dropped > 0 || repaired > 0 {
            info!(
                "Session {}: dropped {} messages, repaired {}",
                session.id, dropped, repaired
            );
        }

        // Write validated messages as JSONL
        let mut jsonl_content = String::new();
        for msg in &valid_messages {
            let line = serde_json::to_string(msg).map_err(std::io::Error::other)?;
            jsonl_content.push_str(&line);
            jsonl_content.push('\n');
        }

        let tmp_jsonl = self.session_dir.join(format!(".{}.jsonl.tmp", session.id));
        std::fs::write(&tmp_jsonl, &jsonl_content)?;
        std::fs::rename(&tmp_jsonl, &jsonl_path)?;

        // Update index
        if let Err(e) = self.index.upsert_entry(session) {
            warn!("Failed to update session index: {}", e);
        }

        debug!(
            "Saved session {} ({} messages, {} written after validation)",
            session.id,
            session.messages.len(),
            valid_messages.len()
        );
        Ok(())
    }

    /// Save the current session.
    pub fn save_current(&self) -> std::io::Result<()> {
        if let Some(session) = &self.current_session {
            self.save_session(session)
        } else {
            Ok(())
        }
    }

    /// Load a session from disk.
    ///
    /// Reads from both the JSON metadata file and JSONL transcript.
    /// Falls back to reading messages from the JSON file for legacy format.
    pub fn load_session(&self, session_id: &str) -> std::io::Result<Session> {
        let json_path = self.session_dir.join(format!("{session_id}.json"));
        self.load_from_file(&json_path)
    }

    /// Load a session from a specific file path.
    pub fn load_from_file(&self, json_path: &Path) -> std::io::Result<Session> {
        let content = std::fs::read_to_string(json_path)?;
        let mut session: Session = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Try to load messages from JSONL file
        let jsonl_path = json_path.with_extension("jsonl");
        if jsonl_path.exists() {
            let jsonl_content = std::fs::read_to_string(&jsonl_path)?;
            let mut messages = Vec::new();
            for line in jsonl_content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str(line) {
                    Ok(msg) => messages.push(msg),
                    Err(e) => {
                        warn!("Skipping invalid JSONL line: {}", e);
                    }
                }
            }
            if !messages.is_empty() {
                let (dropped, repaired) = filter_and_repair_messages(&mut messages);
                if dropped > 0 || repaired > 0 {
                    info!(
                        "Loaded session: repaired {} messages, dropped {}",
                        repaired, dropped
                    );
                }
                session.messages = messages;
            }
        }
        // If no JSONL file, messages from the JSON file are used (legacy format)

        debug!(
            "Loaded session {} ({} messages)",
            session.id,
            session.messages.len()
        );
        Ok(session)
    }

    /// Load a session and set it as current.
    pub fn resume_session(&mut self, session_id: &str) -> std::io::Result<&Session> {
        let session = self.load_session(session_id)?;
        self.current_session = Some(session);
        // SAFETY: we just set current_session to Some on the line above
        Ok(self
            .current_session
            .as_ref()
            .expect("current_session was just set to Some"))
    }

    /// Get the session index.
    pub fn index(&self) -> &SessionIndex {
        &self.index
    }

    /// Store a string value in the current session's metadata.
    ///
    /// Useful for persisting mode, thinking level, autonomy level, etc.
    pub fn set_metadata(&mut self, key: &str, value: &str) {
        let Some(session) = &mut self.current_session else {
            return;
        };

        let json_value = serde_json::Value::String(value.to_string());
        session.metadata.insert(key.to_string(), json_value.clone());
        let session_id = session.id.clone();

        self.emit_event(
            &session_id,
            SessionEvent::MetadataUpdated {
                key: key.to_string(),
                value: json_value,
            },
        );
    }

    /// Read a string value from the current session's metadata.
    pub fn get_metadata(&self, key: &str) -> Option<String> {
        self.current_session
            .as_ref()
            .and_then(|s| s.metadata.get(key))
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    /// Emit a session event to the event store if configured.
    ///
    /// Failures are logged as warnings but never propagated -- the JSON
    /// persistence is the source of truth, events are a sidecar.
    pub(crate) fn emit_event(&self, aggregate_id: &str, event: SessionEvent) {
        if let Some(store) = &self.event_store
            && let Err(e) = store.append(aggregate_id, vec![event])
        {
            warn!("Failed to emit event to event store: {}", e);
        }
    }
}

#[cfg(test)]
mod tests;
