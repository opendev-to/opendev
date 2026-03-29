//! Autocomplete popup controller for the TUI.
//!
//! Manages a popup overlay showing completion suggestions.

/// A single completion item.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// The text to insert on selection.
    pub text: String,
    /// Display label (may differ from insertion text).
    pub label: String,
    /// Optional description shown alongside the label.
    pub description: Option<String>,
}

/// Controller for the autocomplete popup overlay.
pub struct AutocompletePopupController {
    items: Vec<CompletionItem>,
    selected: usize,
    visible: bool,
}

impl AutocompletePopupController {
    /// Create a new hidden autocomplete popup.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            visible: false,
        }
    }

    /// Whether the popup is currently visible.
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// The completion items.
    pub fn items(&self) -> &[CompletionItem] {
        &self.items
    }

    /// The currently selected index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Show the popup with the given completion items.
    pub fn show(&mut self, items: Vec<CompletionItem>) {
        self.items = items;
        self.selected = 0;
        self.visible = !self.items.is_empty();
    }

    /// Hide the popup.
    pub fn hide(&mut self) {
        self.visible = false;
        self.items.clear();
        self.selected = 0;
    }

    /// Move selection to the next item (wrapping).
    pub fn next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.items.len();
    }

    /// Move selection to the previous item (wrapping).
    pub fn prev(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + self.items.len() - 1) % self.items.len();
    }

    /// Confirm the current selection and hide the popup.
    ///
    /// Returns `None` if items list is empty or popup is hidden.
    pub fn select(&mut self) -> Option<&CompletionItem> {
        if !self.visible || self.items.is_empty() {
            return None;
        }
        self.visible = false;
        Some(&self.items[self.selected])
    }
}

impl Default for AutocompletePopupController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "autocomplete_popup_tests.rs"]
mod tests;
