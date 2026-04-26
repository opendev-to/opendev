//! Command history with frecency-based ranking.
//!
//! Stores user input history to `~/.opendev/history.json` and supports
//! Up/Down arrow navigation through previous commands.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::warn;

/// Maximum number of history entries to keep on disk.
const MAX_HISTORY_ENTRIES: usize = 500;

/// Persisted history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryEntry {
    /// The command text.
    text: String,
    /// How many times this command has been used.
    count: u32,
    /// Unix timestamp of last use (seconds since epoch).
    last_used: u64,
}

/// Command history with Up/Down arrow navigation.
///
/// Entries are ordered by frecency (frequency * recency) and can be
/// navigated with [`navigate_up`] and [`navigate_down`].
#[derive(Debug)]
pub struct CommandHistory {
    /// All history entries, ordered most-recent-first.
    entries: Vec<HistoryEntry>,
    /// Current navigation index (`None` = user is typing fresh input).
    nav_index: Option<usize>,
    /// The text the user was typing before they started navigating.
    saved_input: String,
    /// Path to the history file on disk.
    file_path: PathBuf,
}

impl CommandHistory {
    /// Create a new command history, loading from `~/.opendev/history.json`
    /// if it exists.
    pub fn new() -> Self {
        let file_path = Self::default_path();
        let entries = Self::load_from_file(&file_path);
        Self {
            entries,
            nav_index: None,
            saved_input: String::new(),
            file_path,
        }
    }

    /// Create a command history backed by a specific file path (for testing).
    pub fn with_path(file_path: PathBuf) -> Self {
        let entries = Self::load_from_file(&file_path);
        Self {
            entries,
            nav_index: None,
            saved_input: String::new(),
            file_path,
        }
    }

    /// Record a command in the history.
    ///
    /// If the command already exists, its count and timestamp are updated.
    /// Otherwise it is inserted at the front.
    pub fn record(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(entry) = self.entries.iter_mut().find(|e| e.text == text) {
            entry.count += 1;
            entry.last_used = now;
        } else {
            self.entries.insert(
                0,
                HistoryEntry {
                    text: text.to_string(),
                    count: 1,
                    last_used: now,
                },
            );
        }

        // Sort by last_used descending (most recent first)
        self.entries.sort_by_key(|e| std::cmp::Reverse(e.last_used));

        // Trim to max size
        if self.entries.len() > MAX_HISTORY_ENTRIES {
            self.entries.truncate(MAX_HISTORY_ENTRIES);
        }

        // Reset navigation
        self.nav_index = None;
        self.saved_input.clear();

        // Persist
        self.save();
    }

    /// Navigate up (older) in history.
    ///
    /// `current_input` is the text currently in the input buffer. On the
    /// first Up press it is saved so the user can return to it with Down.
    ///
    /// Returns the history entry text to display, or `None` if at the end.
    pub fn navigate_up(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        match self.nav_index {
            None => {
                // First press: save current input and show most recent entry
                self.saved_input = current_input.to_string();
                self.nav_index = Some(0);
                Some(&self.entries[0].text)
            }
            Some(idx) => {
                let next = idx + 1;
                if next < self.entries.len() {
                    self.nav_index = Some(next);
                    Some(&self.entries[next].text)
                } else {
                    // Already at the oldest entry
                    Some(&self.entries[idx].text)
                }
            }
        }
    }

    /// Navigate down (newer) in history.
    ///
    /// Returns the history entry text, or the saved input if the user has
    /// scrolled past the most recent entry.
    pub fn navigate_down(&mut self) -> Option<&str> {
        match self.nav_index {
            None => None,
            Some(0) => {
                // Back to the user's original input
                self.nav_index = None;
                Some(&self.saved_input)
            }
            Some(idx) => {
                let prev = idx - 1;
                self.nav_index = Some(prev);
                Some(&self.entries[prev].text)
            }
        }
    }

    /// Reset navigation state (e.g. when the user starts typing).
    pub fn reset_navigation(&mut self) {
        self.nav_index = None;
        self.saved_input.clear();
    }

    /// Whether the user is currently navigating history.
    pub fn is_navigating(&self) -> bool {
        self.nav_index.is_some()
    }

    /// Number of entries in the history.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn default_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".opendev").join("history.json")
    }

    fn load_from_file(path: &PathBuf) -> Vec<HistoryEntry> {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn save(&self) {
        if let Some(parent) = self.file_path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            warn!("Failed to create history directory: {}", e);
            return;
        }
        match serde_json::to_string_pretty(&self.entries) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.file_path, json) {
                    warn!("Failed to write history file: {}", e);
                }
            }
            Err(e) => {
                warn!("Failed to serialize history: {}", e);
            }
        }
    }
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
