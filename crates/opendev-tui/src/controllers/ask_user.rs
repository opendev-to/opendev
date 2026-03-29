//! Ask-user prompt controller for the TUI.
//!
//! Displays a question with numbered options and tracks the user's selection.
//! The key handler is responsible for sending the answer through the response
//! channel stored in `App::ask_user_response_tx`.

/// Controller for displaying questions with selectable options or free-text input.
pub struct AskUserController {
    question: String,
    options: Vec<String>,
    default: Option<String>,
    selected: usize,
    active: bool,
    text_input: String,
}

impl AskUserController {
    /// Create a new inactive ask-user controller.
    pub fn new() -> Self {
        Self {
            question: String::new(),
            options: Vec::new(),
            default: None,
            selected: 0,
            active: false,
            text_input: String::new(),
        }
    }

    /// Whether the prompt is currently active.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The question being asked.
    pub fn question(&self) -> &str {
        &self.question
    }

    /// The available options.
    pub fn options(&self) -> &[String] {
        &self.options
    }

    /// The currently selected index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// The default value (used as fallback on cancel/Esc).
    pub fn default_value(&self) -> Option<String> {
        self.default.clone()
    }

    /// Whether the prompt has selectable options.
    pub fn has_options(&self) -> bool {
        !self.options.is_empty()
    }

    /// The current free-text input buffer.
    pub fn text_input(&self) -> &str {
        &self.text_input
    }

    /// Append a character to the free-text input.
    pub fn push_char(&mut self, c: char) {
        self.text_input.push(c);
    }

    /// Remove the last character from the free-text input.
    pub fn pop_char(&mut self) {
        self.text_input.pop();
    }

    /// Start the ask-user prompt.
    pub fn start(&mut self, question: String, options: Vec<String>, default: Option<String>) {
        self.question = question;
        self.options = options;
        self.default = default;
        self.selected = 0;
        self.active = true;
    }

    /// Move selection to the next option (wrapping).
    pub fn next(&mut self) {
        if !self.active || self.options.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.options.len();
    }

    /// Move selection to the previous option (wrapping).
    pub fn prev(&mut self) {
        if !self.active || self.options.is_empty() {
            return;
        }
        self.selected = (self.selected + self.options.len() - 1) % self.options.len();
    }

    /// Confirm the current selection and deactivate.
    ///
    /// When options are present, returns the selected option text.
    /// When no options exist, returns the free-text input (or default if input is empty).
    /// Returns `None` only if there is nothing to confirm.
    pub fn confirm(&mut self) -> Option<String> {
        if !self.active {
            return None;
        }

        if !self.options.is_empty() {
            let answer = self.options[self.selected].clone();
            self.cleanup();
            return Some(answer);
        }

        // Free-text mode: use text input, fall back to default
        let answer = if self.text_input.is_empty() {
            self.default.clone()
        } else {
            Some(self.text_input.clone())
        };

        if answer.is_some() {
            self.cleanup();
        }
        answer
    }

    /// Cancel the prompt and deactivate.
    /// The caller is responsible for sending the fallback through the response channel.
    pub fn cancel(&mut self) {
        if !self.active {
            return;
        }
        self.cleanup();
    }

    /// Reset to inactive state.
    fn cleanup(&mut self) {
        self.active = false;
        self.question.clear();
        self.options.clear();
        self.default = None;
        self.selected = 0;
        self.text_input.clear();
    }
}

impl Default for AskUserController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "ask_user_tests.rs"]
mod tests;
