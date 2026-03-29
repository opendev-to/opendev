//! Ask-user channel types shared between tools and TUI.
//!
//! `AskUserRequest` lives here so that both `opendev-tools-impl`
//! (which blocks inside `AskUserTool::execute()`) and `opendev-tui`
//! (which renders the ask-user panel) can reference it without a
//! circular dependency.

use tokio::sync::{mpsc, oneshot};

/// A request sent from `AskUserTool` to the TUI for user input.
///
/// The tool creates a oneshot channel, sends this struct through an mpsc
/// channel, and then awaits the oneshot receiver. The TUI displays the
/// question, collects the user's answer, and sends it back via `response_tx`.
#[derive(Debug)]
pub struct AskUserRequest {
    /// The question to display.
    pub question: String,
    /// Optional list of choices.
    pub options: Vec<String>,
    /// Default answer if the user cancels.
    pub default: Option<String>,
    /// Oneshot sender the TUI uses to return the user's answer.
    pub response_tx: oneshot::Sender<String>,
}

/// Convenience type alias for the sender half that `AskUserTool` holds.
pub type AskUserSender = mpsc::UnboundedSender<AskUserRequest>;

/// Convenience type alias for the receiver half that the TUI polls.
pub type AskUserReceiver = mpsc::UnboundedReceiver<AskUserRequest>;

/// Create a paired (sender, receiver) for ask-user communication.
pub fn ask_user_channel() -> (AskUserSender, AskUserReceiver) {
    mpsc::unbounded_channel()
}

#[cfg(test)]
#[path = "ask_user_channel_tests.rs"]
mod tests;
