//! Session management models.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::file_change::{FileChange, FileChangeType};
use crate::message::ChatMessage;

/// Session metadata for listing and searching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    #[serde(with = "crate::datetime_compat")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "crate::datetime_compat")]
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub has_session_model: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,

    // Summary stats
    #[serde(default)]
    pub summary_additions: u64,
    #[serde(default)]
    pub summary_deletions: u64,
    #[serde(default)]
    pub summary_files: u64,

    // Multi-channel fields
    #[serde(default = "default_channel")]
    pub channel: String,
    #[serde(default)]
    pub channel_user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

fn default_channel() -> String {
    "cli".to_string()
}

fn generate_session_id() -> String {
    Uuid::new_v4().to_string()[..12].to_string()
}

/// Represents a conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    #[serde(default = "generate_session_id")]
    pub id: String,
    #[serde(default = "Utc::now", with = "crate::datetime_compat")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now", with = "crate::datetime_compat")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub context_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    /// Serialized ACE Playbook.
    #[serde(default)]
    pub playbook: Option<HashMap<String, serde_json::Value>>,
    /// Track file changes in this session.
    #[serde(default)]
    pub file_changes: Vec<FileChange>,

    // Multi-channel fields
    #[serde(default = "default_channel")]
    pub channel: String,
    #[serde(default = "default_chat_type")]
    pub chat_type: String,
    #[serde(default)]
    pub channel_user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub delivery_context: HashMap<String, serde_json::Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::datetime_compat::option"
    )]
    pub last_activity: Option<DateTime<Utc>>,
    #[serde(default)]
    pub workspace_confirmed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    /// ID of parent session (if forked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// tool_call_id -> child session_id
    #[serde(default)]
    pub subagent_sessions: HashMap<String, String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::datetime_compat::option"
    )]
    pub time_archived: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
}

fn default_chat_type() -> String {
    "direct".to_string()
}

impl Session {
    /// Create a new session with defaults.
    pub fn new() -> Self {
        Self {
            id: generate_session_id(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            messages: Vec::new(),
            context_files: Vec::new(),
            working_directory: None,
            metadata: HashMap::new(),
            playbook: Some(HashMap::new()),
            file_changes: Vec::new(),
            channel: "cli".to_string(),
            chat_type: "direct".to_string(),
            channel_user_id: String::new(),
            thread_id: None,
            delivery_context: HashMap::new(),
            last_activity: None,
            workspace_confirmed: false,
            owner_id: None,
            parent_id: None,
            subagent_sessions: HashMap::new(),
            time_archived: None,
            slug: None,
        }
    }

    /// Total lines added across all file changes.
    pub fn summary_additions(&self) -> u64 {
        self.file_changes.iter().map(|fc| fc.lines_added).sum()
    }

    /// Total lines removed across all file changes.
    pub fn summary_deletions(&self) -> u64 {
        self.file_changes.iter().map(|fc| fc.lines_removed).sum()
    }

    /// Number of unique files changed.
    pub fn summary_files(&self) -> usize {
        let unique: std::collections::HashSet<&str> = self
            .file_changes
            .iter()
            .map(|fc| fc.file_path.as_str())
            .collect();
        unique.len()
    }

    /// Soft-archive this session.
    pub fn archive(&mut self) {
        self.time_archived = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Restore an archived session.
    pub fn unarchive(&mut self) {
        self.time_archived = None;
        self.updated_at = Utc::now();
    }

    /// Check if session is archived.
    pub fn is_archived(&self) -> bool {
        self.time_archived.is_some()
    }

    /// Generate URL-friendly slug from title.
    pub fn generate_slug(&self, title: Option<&str>) -> String {
        let text = title
            .or_else(|| self.metadata.get("title").and_then(|v| v.as_str()))
            .unwrap_or("");

        if text.is_empty() {
            return self.id[..self.id.len().min(8)].to_string();
        }

        let re = Regex::new(r"[^a-z0-9]+").unwrap();
        let lowered = text.to_lowercase();
        let slug = re.replace_all(&lowered, "-");
        let slug = slug.trim_matches('-');
        let slug = if slug.len() > 50 {
            slug[..50].trim_end_matches('-')
        } else {
            slug
        };

        if slug.is_empty() {
            self.id[..self.id.len().min(8)].to_string()
        } else {
            slug.to_string()
        }
    }

    /// Add a file change to the session.
    pub fn add_file_change(&mut self, file_change: FileChange) {
        // Check if this is a modification of an existing file
        for existing in &mut self.file_changes {
            if existing.file_path == file_change.file_path
                && existing.change_type == FileChangeType::Modified
                && file_change.change_type == FileChangeType::Modified
            {
                existing.lines_added += file_change.lines_added;
                existing.lines_removed += file_change.lines_removed;
                existing.timestamp = file_change.timestamp;
                existing.description = file_change.description.clone();
                return;
            }
        }

        // Remove any previous change for the same file (for non-modifications)
        self.file_changes
            .retain(|fc| fc.file_path != file_change.file_path);

        let mut fc = file_change;
        fc.session_id = Some(self.id.clone());
        self.file_changes.push(fc);
        self.updated_at = Utc::now();
    }

    /// Get a summary of file changes in this session.
    pub fn get_file_changes_summary(&self) -> FileChangesSummary {
        let created = self
            .file_changes
            .iter()
            .filter(|fc| fc.change_type == FileChangeType::Created)
            .count();
        let modified = self
            .file_changes
            .iter()
            .filter(|fc| fc.change_type == FileChangeType::Modified)
            .count();
        let deleted = self
            .file_changes
            .iter()
            .filter(|fc| fc.change_type == FileChangeType::Deleted)
            .count();
        let renamed = self
            .file_changes
            .iter()
            .filter(|fc| fc.change_type == FileChangeType::Renamed)
            .count();
        let total_lines_added: u64 = self.file_changes.iter().map(|fc| fc.lines_added).sum();
        let total_lines_removed: u64 = self.file_changes.iter().map(|fc| fc.lines_removed).sum();

        FileChangesSummary {
            total: self.file_changes.len(),
            created,
            modified,
            deleted,
            renamed,
            total_lines_added,
            total_lines_removed,
            net_lines: total_lines_added as i64 - total_lines_removed as i64,
        }
    }

    /// Calculate total token count.
    pub fn total_tokens(&self) -> u64 {
        self.messages.iter().map(|msg| msg.token_estimate()).sum()
    }

    /// Get session metadata.
    pub fn get_metadata(&self) -> SessionMetadata {
        SessionMetadata {
            id: self.id.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            message_count: self.messages.len(),
            total_tokens: self.total_tokens(),
            title: self
                .metadata
                .get("title")
                .and_then(|v| v.as_str())
                .map(String::from),
            summary: self
                .metadata
                .get("summary")
                .and_then(|v| v.as_str())
                .map(String::from),
            tags: self
                .metadata
                .get("tags")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            working_directory: self.working_directory.clone(),
            has_session_model: false,
            owner_id: self.owner_id.clone(),
            summary_additions: self.summary_additions(),
            summary_deletions: self.summary_deletions(),
            summary_files: self.summary_files() as u64,
            channel: self.channel.clone(),
            channel_user_id: self.channel_user_id.clone(),
            thread_id: self.thread_id.clone(),
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of file changes in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangesSummary {
    pub total: usize,
    pub created: usize,
    pub modified: usize,
    pub deleted: usize,
    pub renamed: usize,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub net_lines: i64,
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
