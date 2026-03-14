//! Ask-user prompt controller for the TUI.
//!
//! Mirrors `PlanApprovalController` — displays a question with numbered
//! options and collects the user's selection via a oneshot channel.

use tokio::sync::oneshot;

/// Controller for displaying questions with selectable options.
pub struct AskUserController {
    question: String,
    options: Vec<String>,
    default: Option<String>,
    selected: usize,
    active: bool,
    response_tx: Option<oneshot::Sender<String>>,
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
            response_tx: None,
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

    /// Start the ask-user prompt.
    ///
    /// Returns a receiver that will yield the user's answer.
    pub fn start(
        &mut self,
        question: String,
        options: Vec<String>,
        default: Option<String>,
    ) -> oneshot::Receiver<String> {
        self.question = question;
        self.options = options;
        self.default = default;
        self.selected = 0;
        self.active = true;

        let (tx, rx) = oneshot::channel();
        self.response_tx = Some(tx);
        rx
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
    /// Returns the selected option text, or `None` if options list is empty.
    pub fn confirm(&mut self) -> Option<String> {
        if !self.active || self.options.is_empty() {
            return None;
        }

        let answer = self.options[self.selected].clone();

        if let Some(tx) = self.response_tx.take() {
            let _ = tx.send(answer.clone());
        }

        self.cleanup();
        Some(answer)
    }

    /// Cancel the prompt — sends the default (or empty string).
    pub fn cancel(&mut self) {
        if !self.active {
            return;
        }
        let fallback = self.default.clone().unwrap_or_default();
        if let Some(tx) = self.response_tx.take() {
            let _ = tx.send(fallback);
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
        self.response_tx = None;
    }
}

impl Default for AskUserController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_options() -> Vec<String> {
        vec!["Rust".into(), "Python".into(), "Go".into()]
    }

    #[test]
    fn test_new_is_inactive() {
        let ctrl = AskUserController::new();
        assert!(!ctrl.active());
    }

    #[tokio::test]
    async fn test_start_activates() {
        let mut ctrl = AskUserController::new();
        let _rx = ctrl.start("Pick a language?".into(), sample_options(), None);
        assert!(ctrl.active());
        assert_eq!(ctrl.options().len(), 3);
        assert_eq!(ctrl.selected_index(), 0);
        assert!(ctrl.question().contains("language"));
    }

    #[tokio::test]
    async fn test_confirm_sends_selection() {
        let mut ctrl = AskUserController::new();
        let rx = ctrl.start("Pick?".into(), sample_options(), None);
        ctrl.next(); // index 1 = "Python"
        let answer = ctrl.confirm().unwrap();
        assert_eq!(answer, "Python");
        assert!(!ctrl.active());

        let received = rx.await.unwrap();
        assert_eq!(received, "Python");
    }

    #[tokio::test]
    async fn test_cancel_sends_default() {
        let mut ctrl = AskUserController::new();
        let rx = ctrl.start("Pick?".into(), sample_options(), Some("Go".into()));
        ctrl.cancel();
        assert!(!ctrl.active());

        let received = rx.await.unwrap();
        assert_eq!(received, "Go");
    }

    #[tokio::test]
    async fn test_cancel_no_default_sends_empty() {
        let mut ctrl = AskUserController::new();
        let rx = ctrl.start("Pick?".into(), sample_options(), None);
        ctrl.cancel();

        let received = rx.await.unwrap();
        assert_eq!(received, "");
    }

    #[tokio::test]
    async fn test_next_prev_wraps() {
        let mut ctrl = AskUserController::new();
        let _rx = ctrl.start("Q?".into(), sample_options(), None);

        ctrl.next();
        assert_eq!(ctrl.selected_index(), 1);
        ctrl.next();
        ctrl.next();
        assert_eq!(ctrl.selected_index(), 0); // wrap

        ctrl.prev();
        assert_eq!(ctrl.selected_index(), 2); // wrap back
    }

    #[test]
    fn test_confirm_empty_options() {
        let mut ctrl = AskUserController::new();
        let _rx = ctrl.start("Q?".into(), vec![], None);
        assert!(ctrl.confirm().is_none());
    }
}
