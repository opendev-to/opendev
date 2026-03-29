//! Input message history with up/down navigation.
//!
//! Mirrors Python's `MessageHistory` from
//! `opendev/ui_textual/managers/message_history.py`.

/// Manages a bounded history of sent messages with cursor-based navigation.
pub struct MessageHistory {
    history: Vec<String>,
    cursor: usize,
    capacity: usize,
    /// Tracks whether the cursor is active (user has navigated).
    navigating: bool,
}

impl MessageHistory {
    /// Create a new message history with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            history: Vec::new(),
            cursor: 0,
            capacity,
            navigating: false,
        }
    }

    /// Push a new message onto the history.
    ///
    /// If capacity is exceeded, the oldest message is dropped.
    pub fn push(&mut self, msg: String) {
        if msg.is_empty() {
            return;
        }
        // Avoid consecutive duplicates
        if self.history.last().map(|s| s.as_str()) == Some(&msg) {
            self.reset_cursor();
            return;
        }
        self.history.push(msg);
        if self.history.len() > self.capacity {
            self.history.remove(0);
        }
        self.reset_cursor();
    }

    /// Navigate up (to older messages).
    ///
    /// Returns the message at the new cursor position, or `None` if
    /// there is no history.
    pub fn up(&mut self) -> Option<&str> {
        if self.history.is_empty() {
            return None;
        }
        if !self.navigating {
            self.navigating = true;
            self.cursor = self.history.len() - 1;
        } else if self.cursor > 0 {
            self.cursor -= 1;
        }
        Some(&self.history[self.cursor])
    }

    /// Navigate down (to newer messages).
    ///
    /// Returns the message at the new cursor position, or `None` if
    /// already past the newest entry.
    pub fn down(&mut self) -> Option<&str> {
        if !self.navigating || self.history.is_empty() {
            return None;
        }
        if self.cursor < self.history.len() - 1 {
            self.cursor += 1;
            Some(&self.history[self.cursor])
        } else {
            self.navigating = false;
            None
        }
    }

    /// Reset the navigation cursor.
    pub fn reset_cursor(&mut self) {
        self.cursor = 0;
        self.navigating = false;
    }

    /// The number of messages in history.
    pub fn len(&self) -> usize {
        self.history.len()
    }

    /// Whether the history is empty.
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
}

#[cfg(test)]
#[path = "message_history_tests.rs"]
mod tests;
