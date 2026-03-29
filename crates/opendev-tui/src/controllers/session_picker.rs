//! Session picker controller for selecting past sessions in the TUI.
//!
//! Provides a searchable session selection popup.

/// A session option displayed in the picker.
#[derive(Debug, Clone)]
pub struct SessionOption {
    /// Session identifier.
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Last updated timestamp (formatted string).
    pub updated_at: String,
    /// Number of messages in the session.
    pub message_count: usize,
}

/// Controller for navigating and selecting a session from a list.
pub struct SessionPickerController {
    /// All available sessions (unfiltered).
    all_sessions: Vec<SessionOption>,
    /// Filtered sessions matching the current search query.
    filtered_sessions: Vec<usize>,
    /// Current selected index into `filtered_sessions`.
    selected_index: usize,
    /// Whether the picker is currently active.
    active: bool,
    /// Current search/filter query.
    search_query: String,
    /// Scroll offset for the visible window.
    scroll_offset: usize,
    /// Maximum visible items in the popup.
    max_visible: usize,
}

impl Default for SessionPickerController {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionPickerController {
    /// Create a new picker with the given session options.
    pub fn new() -> Self {
        Self {
            all_sessions: Vec::new(),
            filtered_sessions: Vec::new(),
            selected_index: 0,
            active: true,
            search_query: String::new(),
            scroll_offset: 0,
            max_visible: 15,
        }
    }

    /// Create a picker pre-populated with session options.
    pub fn from_sessions(sessions: Vec<SessionOption>) -> Self {
        let filtered: Vec<usize> = (0..sessions.len()).collect();
        Self {
            all_sessions: sessions,
            filtered_sessions: filtered,
            selected_index: 0,
            active: true,
            search_query: String::new(),
            scroll_offset: 0,
            max_visible: 15,
        }
    }

    /// Whether the picker is currently active.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The filtered session options to display.
    pub fn visible_sessions(&self) -> Vec<(usize, &SessionOption)> {
        self.filtered_sessions
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(self.max_visible)
            .map(|(i, &session_idx)| (i, &self.all_sessions[session_idx]))
            .collect()
    }

    /// Total number of filtered sessions.
    pub fn filtered_count(&self) -> usize {
        self.filtered_sessions.len()
    }

    /// The currently selected index in the filtered list.
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// The current search query.
    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    /// Move selection to the next item (wrapping).
    pub fn next(&mut self) {
        if self.filtered_sessions.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.filtered_sessions.len();
        self.ensure_visible();
    }

    /// Move selection to the previous item (wrapping).
    pub fn prev(&mut self) {
        if self.filtered_sessions.is_empty() {
            return;
        }
        self.selected_index =
            (self.selected_index + self.filtered_sessions.len() - 1) % self.filtered_sessions.len();
        self.ensure_visible();
    }

    /// Ensure the selected item is within the visible scroll window.
    fn ensure_visible(&mut self) {
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + self.max_visible {
            self.scroll_offset = self.selected_index + 1 - self.max_visible;
        }
    }

    /// Confirm the current selection and deactivate the picker.
    ///
    /// Returns `None` if the filtered list is empty.
    pub fn select(&mut self) -> Option<SessionOption> {
        if self.filtered_sessions.is_empty() {
            return None;
        }
        self.active = false;
        let session_idx = self.filtered_sessions[self.selected_index];
        Some(self.all_sessions[session_idx].clone())
    }

    /// Cancel the picker without selecting.
    pub fn cancel(&mut self) {
        self.active = false;
    }

    /// Add a character to the search query and re-filter.
    pub fn search_push(&mut self, c: char) {
        self.search_query.push(c);
        self.refilter();
    }

    /// Remove the last character from the search query and re-filter.
    pub fn search_pop(&mut self) {
        self.search_query.pop();
        self.refilter();
    }

    /// Re-filter sessions based on the current search query.
    fn refilter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_sessions = (0..self.all_sessions.len()).collect();
        } else {
            let query = self.search_query.to_lowercase();
            self.filtered_sessions = self
                .all_sessions
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.title.to_lowercase().contains(&query) || s.id.to_lowercase().contains(&query)
                })
                .map(|(i, _)| i)
                .collect();
        }
        self.selected_index = 0;
        self.scroll_offset = 0;
    }
}

#[cfg(test)]
#[path = "session_picker_tests.rs"]
mod tests;
