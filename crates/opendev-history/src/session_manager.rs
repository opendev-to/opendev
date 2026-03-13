//! Session manager: JSON read/write to ~/.opendev/sessions/.
//!
//! Handles session file I/O, including reading from both legacy JSON format
//! (all-in-one) and the newer JSON+JSONL split format.

use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use opendev_models::{Role, Session};

use crate::index::SessionIndex;

/// Session manager for persisting and loading sessions.
pub struct SessionManager {
    session_dir: PathBuf,
    index: SessionIndex,
    current_session: Option<Session>,
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
        })
    }

    /// Get the session directory.
    pub fn session_dir(&self) -> &Path {
        &self.session_dir
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
        self.current_session = Some(session);
        // SAFETY: we just set current_session to Some on the line above
        self.current_session
            .as_ref()
            .expect("current_session was just set to Some")
    }

    /// Save a session to disk.
    ///
    /// Writes session metadata to `{id}.json` and messages to `{id}.jsonl`.
    pub fn save_session(&self, session: &Session) -> std::io::Result<()> {
        let json_path = self.session_dir.join(format!("{}.json", session.id));
        let jsonl_path = self.session_dir.join(format!("{}.jsonl", session.id));

        // Write metadata (session without messages for the JSON file)
        let mut session_for_json = session.clone();
        session_for_json.messages.clear();

        let json_content =
            serde_json::to_string_pretty(&session_for_json).map_err(std::io::Error::other)?;

        // Atomic write for metadata
        let tmp_json = self.session_dir.join(format!(".{}.json.tmp", session.id));
        std::fs::write(&tmp_json, &json_content)?;
        std::fs::rename(&tmp_json, &json_path)?;

        // Write messages as JSONL
        let mut jsonl_content = String::new();
        for msg in &session.messages {
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
            "Saved session {} ({} messages)",
            session.id,
            session.messages.len()
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
        if let Some(session) = &mut self.current_session {
            session.metadata.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }

    /// Read a string value from the current session's metadata.
    pub fn get_metadata(&self, key: &str) -> Option<String> {
        self.current_session
            .as_ref()
            .and_then(|s| s.metadata.get(key))
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    /// Fork a session from a specific message index.
    ///
    /// Loads the source session, copies messages up to `fork_point`
    /// (exclusive), creates a new session with the parent reference, saves
    /// it, and returns the fork.  If `fork_point` is `None`, all messages
    /// are copied.
    pub fn fork_session(
        &self,
        session_id: &str,
        fork_point: Option<usize>,
    ) -> std::io::Result<Session> {
        let source = self.load_session(session_id)?;

        let at_message_index = fork_point.unwrap_or(source.messages.len());

        if at_message_index > source.messages.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "fork_point {} exceeds message count {}",
                    at_message_index,
                    source.messages.len()
                ),
            ));
        }

        let mut forked = Session::new();
        forked.messages = source.messages[..at_message_index].to_vec();
        forked.parent_id = Some(session_id.to_string());
        forked.working_directory = source.working_directory.clone();
        forked.context_files = source.context_files.clone();
        forked.channel = source.channel.clone();
        forked.channel_user_id = source.channel_user_id.clone();

        // Auto-generate title for the fork
        let title = generate_title_from_messages(&forked.messages)
            .unwrap_or_else(|| format!("Fork of {}", session_id));
        forked
            .metadata
            .insert("title".to_string(), serde_json::Value::String(title));

        self.save_session(&forked)?;
        info!(
            "Forked session {} from {} at message {}",
            forked.id, session_id, at_message_index
        );
        Ok(forked)
    }

    /// Archive a session by setting its `time_archived` timestamp.
    pub fn archive_session(&self, session_id: &str) -> std::io::Result<()> {
        let mut session = self.load_session(session_id)?;
        session.archive();
        self.save_session(&session)?;
        info!("Archived session {}", session_id);
        Ok(())
    }

    /// Unarchive a previously archived session.
    pub fn unarchive_session(&self, session_id: &str) -> std::io::Result<()> {
        let mut session = self.load_session(session_id)?;
        session.unarchive();
        self.save_session(&session)?;
        info!("Unarchived session {}", session_id);
        Ok(())
    }

    /// List sessions, optionally including archived ones.
    ///
    /// Delegates to the session index for fast metadata lookups.
    pub fn list_sessions(&self, include_archived: bool) -> Vec<opendev_models::SessionMetadata> {
        let listing = crate::listing::SessionListing::new(self.session_dir.clone());
        listing.list_sessions(None, include_archived)
    }

    /// Revert a session to a given message step.
    ///
    /// Truncates the session's messages to `step` entries (keeping messages
    /// at indices `0..step`) and saves the result.  Returns an error if the
    /// session does not exist or `step` exceeds the current message count.
    pub fn revert_session(&self, session_id: &str, step: usize) -> std::io::Result<()> {
        let mut session = self.load_session(session_id)?;

        if step > session.messages.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "step {} exceeds message count {}",
                    step,
                    session.messages.len()
                ),
            ));
        }

        session.messages.truncate(step);
        session.updated_at = chrono::Utc::now();
        self.save_session(&session)?;
        info!("Reverted session {} to step {}", session_id, step);
        Ok(())
    }

    /// Search all session files for messages matching a query string.
    ///
    /// Returns a list of `(session_id, matching_message_indices)` pairs for
    /// every session that contains at least one message whose content includes
    /// the query (case-insensitive).
    pub fn search_sessions(&self, query: &str) -> Vec<(String, Vec<usize>)> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<(String, Vec<usize>)> = Vec::new();

        let entries = match std::fs::read_dir(&self.session_dir) {
            Ok(e) => e,
            Err(_) => return results,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            // Only look at .json metadata files (skip index, tmp files, etc.)
            let Some(ext) = path.extension() else {
                continue;
            };
            if ext != "json" {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // Skip the sessions-index file
            if stem == "sessions-index" {
                continue;
            }

            let session = match self.load_session(stem) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let matching_indices: Vec<usize> = session
                .messages
                .iter()
                .enumerate()
                .filter(|(_, msg)| msg.content.to_lowercase().contains(&query_lower))
                .map(|(i, _)| i)
                .collect();

            if !matching_indices.is_empty() {
                results.push((session.id, matching_indices));
            }
        }

        results
    }
}

/// Generate a short title from the first user message.
///
/// Takes the first 60 characters of the first user message, truncated at the
/// last word boundary so that words are not cut in half.  Returns `None` if
/// there are no user messages or the first user message is empty.
pub fn generate_title_from_messages(messages: &[opendev_models::ChatMessage]) -> Option<String> {
    let first_user = messages.iter().find(|m| m.role == Role::User)?;
    let text = first_user.content.trim();
    if text.is_empty() {
        return None;
    }
    Some(truncate_at_word_boundary(text, 60))
}

/// Truncate a string to at most `max_chars` characters at a word boundary.
fn truncate_at_word_boundary(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }

    // Find the last space at or before max_chars
    let truncated = &text[..max_chars];
    if let Some(last_space) = truncated.rfind(' ')
        && last_space > 0
    {
        return format!("{}...", &text[..last_space]);
    }

    // No word boundary found; hard-truncate
    format!("{}...", truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use opendev_models::{ChatMessage, Role};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn make_msg(role: Role, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
            tool_calls: vec![],
            tokens: None,
            thinking_trace: None,
            reasoning_content: None,
            token_usage: None,
            provenance: None,
        }
    }

    #[test]
    fn test_create_session() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
        let session = mgr.create_session();
        assert!(!session.id.is_empty());
        assert!(mgr.current_session().is_some());
    }

    #[test]
    fn test_save_and_load_session() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "test-save-load".to_string();
        session.messages.push(make_msg(Role::User, "hello"));
        session.messages.push(make_msg(Role::Assistant, "hi there"));

        mgr.save_session(&session).unwrap();

        let loaded = mgr.load_session("test-save-load").unwrap();
        assert_eq!(loaded.id, "test-save-load");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "hello");
        assert_eq!(loaded.messages[1].content, "hi there");
    }

    #[test]
    fn test_save_updates_index() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "indexed-session".to_string();
        mgr.save_session(&session).unwrap();

        let index = mgr.index().read_index().unwrap();
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries[0].session_id, "indexed-session");
    }

    #[test]
    fn test_resume_session() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "resume-test".to_string();
        session.messages.push(make_msg(Role::User, "hi"));
        mgr.save_session(&session).unwrap();

        mgr.resume_session("resume-test").unwrap();
        let current = mgr.current_session().unwrap();
        assert_eq!(current.id, "resume-test");
        assert_eq!(current.messages.len(), 1);
    }

    #[test]
    fn test_load_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
        let result = mgr.load_session("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_legacy_json_format() {
        // Test loading from legacy format (messages in JSON, no JSONL)
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "legacy-test".to_string();
        session.messages.push(make_msg(Role::User, "old format"));

        // Write as legacy format (all in JSON, no JSONL)
        let json_path = tmp.path().join("legacy-test.json");
        let content = serde_json::to_string_pretty(&session).unwrap();
        std::fs::write(&json_path, content).unwrap();

        let loaded = mgr.load_session("legacy-test").unwrap();
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].content, "old format");
    }

    #[test]
    fn test_set_get_metadata() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
        mgr.create_session();

        // No metadata set yet
        assert!(mgr.get_metadata("mode").is_none());

        // Set and get
        mgr.set_metadata("mode", "PLAN");
        assert_eq!(mgr.get_metadata("mode").as_deref(), Some("PLAN"));

        mgr.set_metadata("thinking_level", "High");
        assert_eq!(mgr.get_metadata("thinking_level").as_deref(), Some("High"));

        mgr.set_metadata("autonomy_level", "Auto");
        assert_eq!(mgr.get_metadata("autonomy_level").as_deref(), Some("Auto"));
    }

    #[test]
    fn test_metadata_persists_across_save_load() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
        mgr.create_session();

        let session_id = mgr.current_session().unwrap().id.clone();

        mgr.set_metadata("mode", "PLAN");
        mgr.set_metadata("thinking_level", "High");
        mgr.set_metadata("autonomy_level", "Manual");
        mgr.save_current().unwrap();

        // Load in a fresh manager
        let mgr2 = SessionManager::new(tmp.path().to_path_buf()).unwrap();
        let loaded = mgr2.load_session(&session_id).unwrap();

        assert_eq!(
            loaded.metadata.get("mode").and_then(|v| v.as_str()),
            Some("PLAN")
        );
        assert_eq!(
            loaded
                .metadata
                .get("thinking_level")
                .and_then(|v| v.as_str()),
            Some("High")
        );
        assert_eq!(
            loaded
                .metadata
                .get("autonomy_level")
                .and_then(|v| v.as_str()),
            Some("Manual")
        );
    }

    // --- Session forking tests ---

    #[test]
    fn test_fork_session() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "parent-sess".to_string();
        session.working_directory = Some("/tmp/project".to_string());
        session
            .messages
            .push(make_msg(Role::User, "first question"));
        session
            .messages
            .push(make_msg(Role::Assistant, "first answer"));
        session
            .messages
            .push(make_msg(Role::User, "second question"));
        session
            .messages
            .push(make_msg(Role::Assistant, "second answer"));
        mgr.save_session(&session).unwrap();

        let forked = mgr.fork_session("parent-sess", Some(2)).unwrap();
        assert_ne!(forked.id, "parent-sess");
        assert_eq!(forked.parent_id.as_deref(), Some("parent-sess"));
        assert_eq!(forked.messages.len(), 2);
        assert_eq!(forked.messages[0].content, "first question");
        assert_eq!(forked.messages[1].content, "first answer");
        assert_eq!(forked.working_directory.as_deref(), Some("/tmp/project"));

        // Verify it was persisted
        let loaded = mgr.load_session(&forked.id).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.parent_id.as_deref(), Some("parent-sess"));
    }

    #[test]
    fn test_fork_session_at_zero() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "fork-zero".to_string();
        session.messages.push(make_msg(Role::User, "hello"));
        mgr.save_session(&session).unwrap();

        let forked = mgr.fork_session("fork-zero", Some(0)).unwrap();
        assert!(forked.messages.is_empty());
        assert_eq!(forked.parent_id.as_deref(), Some("fork-zero"));
    }

    #[test]
    fn test_fork_session_out_of_bounds() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "fork-oob".to_string();
        session.messages.push(make_msg(Role::User, "hello"));
        mgr.save_session(&session).unwrap();

        let result = mgr.fork_session("fork-oob", Some(5));
        assert!(result.is_err());
    }

    #[test]
    fn test_fork_nonexistent_session() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
        let result = mgr.fork_session("no-such-session", Some(0));
        assert!(result.is_err());
    }

    #[test]
    fn test_fork_generates_title() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "title-src".to_string();
        session
            .messages
            .push(make_msg(Role::User, "Implement the new auth flow"));
        session
            .messages
            .push(make_msg(Role::Assistant, "Sure, I will help"));
        mgr.save_session(&session).unwrap();

        let forked = mgr.fork_session("title-src", Some(2)).unwrap();
        let title = forked
            .metadata
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(title, "Implement the new auth flow");
    }

    // --- Title generation tests ---

    #[test]
    fn test_generate_title_short_message() {
        let msgs = vec![make_msg(Role::User, "Fix the login bug")];
        assert_eq!(
            generate_title_from_messages(&msgs),
            Some("Fix the login bug".to_string())
        );
    }

    #[test]
    fn test_generate_title_long_message() {
        let msgs = vec![make_msg(
            Role::User,
            "Please help me refactor the authentication module to use OAuth2 instead of the custom token system we built",
        )];
        let title = generate_title_from_messages(&msgs).unwrap();
        assert!(title.len() <= 63); // 60 + "..."
        assert!(title.ends_with("..."));
    }

    #[test]
    fn test_generate_title_no_user_messages() {
        let msgs = vec![make_msg(Role::Assistant, "Hello")];
        assert!(generate_title_from_messages(&msgs).is_none());
    }

    #[test]
    fn test_generate_title_empty_messages() {
        let msgs: Vec<ChatMessage> = vec![];
        assert!(generate_title_from_messages(&msgs).is_none());
    }

    #[test]
    fn test_generate_title_empty_content() {
        let msgs = vec![make_msg(Role::User, "   ")];
        assert!(generate_title_from_messages(&msgs).is_none());
    }

    #[test]
    fn test_generate_title_exactly_60_chars() {
        // Exactly 60 chars, no truncation needed
        let text = "a]".repeat(30); // 60 chars
        let msgs = vec![make_msg(Role::User, &text)];
        let title = generate_title_from_messages(&msgs).unwrap();
        assert_eq!(title, text);
    }

    // --- Archiving tests ---

    #[test]
    fn test_archive_session() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "archive-test".to_string();
        session.messages.push(make_msg(Role::User, "hello"));
        mgr.save_session(&session).unwrap();

        mgr.archive_session("archive-test").unwrap();

        let loaded = mgr.load_session("archive-test").unwrap();
        assert!(loaded.is_archived());
        assert!(loaded.time_archived.is_some());
    }

    #[test]
    fn test_unarchive_session() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "unarchive-test".to_string();
        mgr.save_session(&session).unwrap();

        mgr.archive_session("unarchive-test").unwrap();
        mgr.unarchive_session("unarchive-test").unwrap();

        let loaded = mgr.load_session("unarchive-test").unwrap();
        assert!(!loaded.is_archived());
    }

    #[test]
    fn test_list_sessions_excludes_archived() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut s1 = Session::new();
        s1.id = "active-sess".to_string();
        mgr.save_session(&s1).unwrap();

        let mut s2 = Session::new();
        s2.id = "archived-sess".to_string();
        mgr.save_session(&s2).unwrap();
        mgr.archive_session("archived-sess").unwrap();

        // Default listing excludes archived
        let active = mgr.list_sessions(false);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "active-sess");

        // Include archived
        let all = mgr.list_sessions(true);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_archive_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
        assert!(mgr.archive_session("nope").is_err());
    }

    // --- Fork with None (copy all) tests ---

    #[test]
    fn test_fork_session_none_copies_all() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "fork-all".to_string();
        session.messages.push(make_msg(Role::User, "msg1"));
        session.messages.push(make_msg(Role::Assistant, "msg2"));
        session.messages.push(make_msg(Role::User, "msg3"));
        mgr.save_session(&session).unwrap();

        let forked = mgr.fork_session("fork-all", None).unwrap();
        assert_eq!(forked.messages.len(), 3);
        assert_eq!(forked.parent_id.as_deref(), Some("fork-all"));
    }

    // --- Session reverting tests ---

    #[test]
    fn test_revert_session() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "revert-test".to_string();
        session.messages.push(make_msg(Role::User, "step 0"));
        session
            .messages
            .push(make_msg(Role::Assistant, "step 1"));
        session.messages.push(make_msg(Role::User, "step 2"));
        session
            .messages
            .push(make_msg(Role::Assistant, "step 3"));
        mgr.save_session(&session).unwrap();

        mgr.revert_session("revert-test", 2).unwrap();

        let loaded = mgr.load_session("revert-test").unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "step 0");
        assert_eq!(loaded.messages[1].content, "step 1");
    }

    #[test]
    fn test_revert_session_to_zero() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "revert-zero".to_string();
        session.messages.push(make_msg(Role::User, "hello"));
        mgr.save_session(&session).unwrap();

        mgr.revert_session("revert-zero", 0).unwrap();

        let loaded = mgr.load_session("revert-zero").unwrap();
        assert!(loaded.messages.is_empty());
    }

    #[test]
    fn test_revert_session_out_of_bounds() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut session = Session::new();
        session.id = "revert-oob".to_string();
        session.messages.push(make_msg(Role::User, "hello"));
        mgr.save_session(&session).unwrap();

        let result = mgr.revert_session("revert-oob", 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_revert_nonexistent_session() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
        assert!(mgr.revert_session("no-such", 0).is_err());
    }

    // --- Cross-session search tests ---

    #[test]
    fn test_search_sessions() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut s1 = Session::new();
        s1.id = "search-1".to_string();
        s1.messages
            .push(make_msg(Role::User, "Fix the login bug"));
        s1.messages
            .push(make_msg(Role::Assistant, "I will fix that"));
        mgr.save_session(&s1).unwrap();

        let mut s2 = Session::new();
        s2.id = "search-2".to_string();
        s2.messages
            .push(make_msg(Role::User, "Add a new feature"));
        mgr.save_session(&s2).unwrap();

        let mut s3 = Session::new();
        s3.id = "search-3".to_string();
        s3.messages
            .push(make_msg(Role::User, "Another login issue"));
        mgr.save_session(&s3).unwrap();

        let results = mgr.search_sessions("login");
        assert_eq!(results.len(), 2);

        // Check that both sessions with "login" are found
        let ids: Vec<&str> = results.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"search-1"));
        assert!(ids.contains(&"search-3"));
    }

    #[test]
    fn test_search_sessions_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut s1 = Session::new();
        s1.id = "case-test".to_string();
        s1.messages
            .push(make_msg(Role::User, "Fix the LOGIN bug"));
        mgr.save_session(&s1).unwrap();

        let results = mgr.search_sessions("login");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "case-test");
    }

    #[test]
    fn test_search_sessions_returns_indices() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut s1 = Session::new();
        s1.id = "idx-test".to_string();
        s1.messages
            .push(make_msg(Role::User, "first message"));
        s1.messages
            .push(make_msg(Role::Assistant, "target keyword here"));
        s1.messages
            .push(make_msg(Role::User, "another target message"));
        mgr.save_session(&s1).unwrap();

        let results = mgr.search_sessions("target");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, vec![1, 2]);
    }

    #[test]
    fn test_search_sessions_no_matches() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

        let mut s1 = Session::new();
        s1.id = "no-match".to_string();
        s1.messages.push(make_msg(Role::User, "hello world"));
        mgr.save_session(&s1).unwrap();

        let results = mgr.search_sessions("nonexistent");
        assert!(results.is_empty());
    }
}
