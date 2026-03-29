//! Plan approval controller for the TUI.
//!
//! Mirrors Python's `PlanApprovalController` from
//! `opendev/ui_textual/controllers/plan_approval_controller.py`.
//!
//! Displays a plan with three action choices and collects the user's decision
//! via a oneshot channel.

use tokio::sync::oneshot;

// Re-export PlanDecision from opendev-runtime (shared with opendev-tools-impl).
pub use opendev_runtime::PlanDecision;

/// Status of a plan under review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    Pending,
    Approved,
    Rejected,
    Modified,
}

/// A single action option in the plan approval prompt.
#[derive(Debug, Clone)]
pub struct PlanAction {
    /// Display label.
    pub label: String,
    /// Description of what this action does.
    pub description: String,
    /// Action key returned in PlanDecision.
    pub action: String,
}

/// Controller for the plan approval prompt state machine.
pub struct PlanApprovalController {
    plan_content: String,
    status: PlanStatus,
    selected_action: usize,
    options: Vec<PlanAction>,
    active: bool,
    response_tx: Option<oneshot::Sender<PlanDecision>>,
}

impl PlanApprovalController {
    /// Create a new inactive plan approval controller.
    pub fn new() -> Self {
        Self {
            plan_content: String::new(),
            status: PlanStatus::Pending,
            selected_action: 0,
            options: Vec::new(),
            active: false,
            response_tx: None,
        }
    }

    /// Whether the prompt is currently active.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The plan content being reviewed.
    pub fn plan_content(&self) -> &str {
        &self.plan_content
    }

    /// Current plan status.
    pub fn status(&self) -> PlanStatus {
        self.status
    }

    /// The available action options.
    pub fn options(&self) -> &[PlanAction] {
        &self.options
    }

    /// The currently selected action index.
    pub fn selected_action(&self) -> usize {
        self.selected_action
    }

    /// Start the plan approval prompt.
    ///
    /// Returns a receiver that will yield the user's decision.
    pub fn start(&mut self, plan_content: String) -> oneshot::Receiver<PlanDecision> {
        self.plan_content = plan_content;
        self.status = PlanStatus::Pending;
        self.selected_action = 0;
        self.active = true;

        self.options = vec![
            PlanAction {
                label: "Start implementation".into(),
                description: "Auto-approve file edits during implementation.".into(),
                action: "approve_auto".into(),
            },
            PlanAction {
                label: "Start implementation (review edits)".into(),
                description: "Review each file edit before it's applied.".into(),
                action: "approve".into(),
            },
            PlanAction {
                label: "Revise plan".into(),
                description: "Stay in plan mode and provide feedback.".into(),
                action: "modify".into(),
            },
        ];

        let (tx, rx) = oneshot::channel();
        self.response_tx = Some(tx);
        rx
    }

    /// Move selection to the next action (wrapping).
    pub fn next(&mut self) {
        if !self.active || self.options.is_empty() {
            return;
        }
        self.selected_action = (self.selected_action + 1) % self.options.len();
    }

    /// Move selection to the previous action (wrapping).
    pub fn prev(&mut self) {
        if !self.active || self.options.is_empty() {
            return;
        }
        self.selected_action = (self.selected_action + self.options.len() - 1) % self.options.len();
    }

    /// Approve the plan (selects the first "approve_auto" action).
    pub fn approve(&mut self) -> Option<PlanDecision> {
        if !self.active {
            return None;
        }
        self.selected_action = 0;
        self.confirm()
    }

    /// Reject the plan (selects "modify" and sends the decision).
    pub fn reject(&mut self) -> Option<PlanDecision> {
        if !self.active {
            return None;
        }
        // Select the last option ("Revise plan")
        self.selected_action = self.options.len().saturating_sub(1);
        self.confirm()
    }

    /// Confirm the currently selected action.
    pub fn confirm(&mut self) -> Option<PlanDecision> {
        if !self.active || self.options.is_empty() {
            return None;
        }

        let option = &self.options[self.selected_action];
        let decision = PlanDecision {
            action: option.action.clone(),
            feedback: String::new(),
        };

        self.status = match option.action.as_str() {
            "approve_auto" | "approve" => PlanStatus::Approved,
            "modify" => PlanStatus::Modified,
            _ => PlanStatus::Rejected,
        };

        if let Some(tx) = self.response_tx.take() {
            let _ = tx.send(decision.clone());
        }

        self.cleanup();
        Some(decision)
    }

    /// Cancel the approval (defaults to "Revise plan").
    pub fn cancel(&mut self) {
        if !self.active {
            return;
        }
        self.selected_action = self.options.len().saturating_sub(1);
        self.confirm();
    }

    /// Reset to inactive state.
    fn cleanup(&mut self) {
        self.active = false;
        self.options.clear();
        self.selected_action = 0;
        self.plan_content.clear();
        self.response_tx = None;
    }
}

impl Default for PlanApprovalController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "plan_approval_tests.rs"]
mod tests;
