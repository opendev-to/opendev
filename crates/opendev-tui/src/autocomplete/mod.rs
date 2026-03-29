//! Autocomplete engine for the TUI input widget.
//!
//! Manages completion state, detects triggers (`/` for commands, `@` for file
//! mentions, Tab for general completion), and renders a popup of ranked
//! completion items.

pub mod completers;
pub mod file_finder;
pub mod formatters;
pub mod strategies;

use crate::controllers::SlashCommand;
use completers::{CommandCompleter, Completer, FileCompleter, SymbolCompleter};
use formatters::CompletionFormatter;
use strategies::CompletionStrategy;

// ── Completion item ────────────────────────────────────────────────

/// The kind of completion a [`CompletionItem`] represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionKind {
    /// A slash command (e.g. `/help`).
    Command,
    /// A file path (triggered by `@`).
    File,
    /// A code symbol.
    Symbol,
}

/// A single completion suggestion.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// Text inserted when the completion is accepted.
    pub insert_text: String,
    /// Short label shown in the popup.
    pub label: String,
    /// Optional description / meta shown to the right.
    pub description: String,
    /// Kind of completion (command, file, symbol).
    pub kind: CompletionKind,
    /// Score used for ranking (higher = better).
    pub score: f64,
}

// ── Trigger detection ──────────────────────────────────────────────

/// Trigger character that activated autocompletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// `/` at the beginning or after whitespace (slash commands).
    Slash,
    /// Slash command argument: the command name has been typed and the user
    /// is now typing an argument. `command` is the command name (without `/`),
    /// and the query is the partial argument text.
    SlashArg { command: String },
    /// `@` for file mentions.
    At,
    /// Tab key for general completion.
    Tab,
}

/// Detect the active trigger and the partial word in `text_before_cursor`.
///
/// Returns `None` when no trigger is active.
pub fn detect_trigger(text_before_cursor: &str) -> Option<(Trigger, String)> {
    // Walk backwards to find the last `@` or `/` not preceded by a non-whitespace char.
    if let Some(pos) = text_before_cursor.rfind('@') {
        // `@` can appear anywhere
        let after_at = &text_before_cursor[pos + 1..];
        // Valid if no spaces in the partial word after @
        if !after_at.contains(' ') {
            return Some((Trigger::At, after_at.to_string()));
        }
    }

    if let Some(pos) = text_before_cursor.rfind('/') {
        // Only trigger if `/` is at position 0 or preceded by whitespace
        let valid_start = pos == 0
            || text_before_cursor
                .as_bytes()
                .get(pos - 1)
                .map(|&b| b == b' ' || b == b'\t' || b == b'\n')
                .unwrap_or(false);
        if valid_start {
            let after_slash = &text_before_cursor[pos + 1..];
            if after_slash.contains(' ') {
                // User has typed a command and a space — argument mode
                let parts: Vec<&str> = after_slash.splitn(2, ' ').collect();
                let command = parts[0].to_string();
                let arg_query = parts.get(1).copied().unwrap_or("").to_string();
                return Some((Trigger::SlashArg { command }, arg_query));
            }
            return Some((Trigger::Slash, after_slash.to_string()));
        }
    }

    None
}

// ── AutocompleteEngine ─────────────────────────────────────────────

/// Central autocomplete engine that drives the popup.
impl std::fmt::Debug for AutocompleteEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutocompleteEngine")
            .field("visible", &self.visible)
            .field("selected", &self.selected)
            .field("items_count", &self.items.len())
            .finish()
    }
}

pub struct AutocompleteEngine {
    command_completer: CommandCompleter,
    file_completer: FileCompleter,
    symbol_completer: SymbolCompleter,
    strategy: CompletionStrategy,

    /// Currently visible completions.
    items: Vec<CompletionItem>,
    /// Index of the selected item inside `items`.
    selected: usize,
    /// Whether the popup is visible.
    visible: bool,
    /// Length of the trigger + query text to delete on accept.
    trigger_len: usize,
}

impl AutocompleteEngine {
    /// Create a new engine rooted at `working_dir`.
    pub fn new(working_dir: std::path::PathBuf) -> Self {
        Self {
            command_completer: CommandCompleter::new(None),
            file_completer: FileCompleter::new(working_dir),
            symbol_completer: SymbolCompleter::new(),
            strategy: CompletionStrategy::default(),
            items: Vec::new(),
            selected: 0,
            visible: false,
            trigger_len: 0,
        }
    }

    /// Update completions based on the text before the cursor.
    ///
    /// Call this on every keystroke (or after a small debounce).
    pub fn update(&mut self, text_before_cursor: &str) {
        match detect_trigger(text_before_cursor) {
            Some((Trigger::Slash, ref query)) => {
                self.items = self.command_completer.complete(query);
                self.strategy.sort(&mut self.items);
                self.selected = 0;
                self.visible = !self.items.is_empty();
                self.trigger_len = 1 + query.len(); // '/' + query
            }
            Some((Trigger::SlashArg { ref command }, ref query)) => {
                self.items = self.command_completer.complete_args(command, query);
                self.strategy.sort(&mut self.items);
                self.selected = 0;
                self.visible = !self.items.is_empty();
                // Delete only the argument portion (not the command)
                self.trigger_len = query.len();
            }
            Some((Trigger::At, ref query)) => {
                self.items = self.file_completer.complete(query);
                self.strategy.sort(&mut self.items);
                self.selected = 0;
                self.visible = !self.items.is_empty();
                self.trigger_len = 1 + query.len(); // '@' + query
            }
            Some((Trigger::Tab, ref query)) => {
                // Tab completion: try files, then symbols
                let mut results = self.file_completer.complete(query);
                results.extend(self.symbol_completer.complete(query));
                self.strategy.sort(&mut results);
                self.items = results;
                self.selected = 0;
                self.visible = !self.items.is_empty();
                self.trigger_len = query.len();
            }
            None => {
                self.dismiss();
            }
        }
    }

    /// Accept the currently selected completion.
    ///
    /// Returns the text to insert and the number of characters to delete
    /// before the cursor (the trigger + partial word).
    pub fn accept(&mut self) -> Option<(String, usize)> {
        if !self.visible || self.items.is_empty() {
            return None;
        }
        let item = &self.items[self.selected];
        let insert = item.insert_text.clone();
        let delete_count = self.trigger_len;
        self.dismiss();
        Some((insert, delete_count))
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if !self.items.is_empty() {
            self.selected = if self.selected == 0 {
                self.items.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    /// Hide the popup.
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.items.clear();
        self.selected = 0;
        self.trigger_len = 0;
    }

    /// Whether the popup is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Currently visible completion items.
    pub fn items(&self) -> &[CompletionItem] {
        &self.items
    }

    /// Index of the selected item.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Render the popup as a list of formatted display lines.
    ///
    /// Each line is `(label, description, is_selected)`.
    pub fn render_popup(&self) -> Vec<(String, String, bool)> {
        self.items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let display = CompletionFormatter::format(item);
                (display.0, display.1, i == self.selected)
            })
            .collect()
    }

    /// Register a frecency access for the given text.
    pub fn record_frecency(&mut self, text: &str) {
        self.strategy.record_access(text);
    }

    /// Add custom slash commands (extends the built-in set).
    pub fn add_commands(&mut self, commands: &[SlashCommand]) {
        self.command_completer.add_commands(commands);
    }

    /// Update the working directory for file completion.
    pub fn set_working_dir(&mut self, dir: std::path::PathBuf) {
        self.file_completer = FileCompleter::new(dir);
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
