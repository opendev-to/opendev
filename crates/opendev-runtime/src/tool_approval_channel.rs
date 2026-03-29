//! Tool approval channel types shared between the react loop and TUI.
//!
//! `ToolApprovalRequest` and `ToolApprovalDecision` live here so that both
//! `opendev-agents` (which gates tool execution) and `opendev-tui`
//! (which renders the approval panel) can reference them without a
//! circular dependency.

use tokio::sync::{mpsc, oneshot};

/// The user's decision on a tool approval request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolApprovalDecision {
    /// Whether the action was approved.
    pub approved: bool,
    /// Which option was selected: "yes", "yes_remember", "no".
    pub choice: String,
    /// The (potentially edited) command text.
    pub command: String,
}

/// A request sent from the react loop to the TUI for tool approval.
///
/// The react loop creates a oneshot channel, sends this struct through an mpsc
/// channel, and then awaits the oneshot receiver. The TUI displays the
/// approval prompt, collects the user's decision, and sends it back via
/// `response_tx`.
#[derive(Debug)]
pub struct ToolApprovalRequest {
    /// Name of the tool requesting approval.
    pub tool_name: String,
    /// The command to be executed.
    pub command: String,
    /// The working directory for the command.
    pub working_dir: String,
    /// Oneshot sender the TUI uses to return the user's decision.
    pub response_tx: oneshot::Sender<ToolApprovalDecision>,
}

/// Convenience type alias for the sender half that the react loop holds.
pub type ToolApprovalSender = mpsc::UnboundedSender<ToolApprovalRequest>;

/// Convenience type alias for the receiver half that the TUI polls.
pub type ToolApprovalReceiver = mpsc::UnboundedReceiver<ToolApprovalRequest>;

/// Create a paired (sender, receiver) for tool approval communication.
pub fn tool_approval_channel() -> (ToolApprovalSender, ToolApprovalReceiver) {
    mpsc::unbounded_channel()
}

#[cfg(test)]
#[path = "tool_approval_channel_tests.rs"]
mod tests;
