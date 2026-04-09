//! UI, status, and miscellaneous event handlers.

use crate::event::AppEvent;
use crate::widgets::toast::{Toast, ToastLevel};

use super::{App, DisplayMessage, DisplayRole, OperationMode};

impl App {
    pub(super) fn handle_budget_exhausted(&mut self, cost_usd: f64, budget_usd: f64) {
        self.state.agent_active = false;
        self.state.messages.push(DisplayMessage::new(
            DisplayRole::System,
            format!(
                "Session cost budget exhausted: ${:.4} spent of ${:.2} budget. \
                 Agent paused. Use /budget to adjust.",
                cost_usd, budget_usd
            ),
        ));
        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_file_change_summary(
        &mut self,
        files: usize,
        additions: u64,
        deletions: u64,
    ) {
        if files > 0 {
            self.state.file_changes = Some((files, additions, deletions));
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_context_usage(&mut self, pct: f64) {
        self.state.context_usage_pct = pct;
        self.state.dirty = true;
    }

    pub(super) fn handle_task_progress_started(&mut self, description: String) {
        self.state.task_progress = Some(crate::widgets::progress::TaskProgress {
            description,
            elapsed_secs: 0,
            token_display: None,
            interrupted: false,
            started_at: std::time::Instant::now(),
        });
        self.state.dirty = true;
    }

    pub(super) fn handle_task_progress_finished(&mut self) {
        self.state.task_progress = None;
        self.state.dirty = true;
    }

    pub(super) fn handle_plan_approval_requested(
        &mut self,
        plan_content: String,
        response_tx: tokio::sync::oneshot::Sender<opendev_runtime::PlanDecision>,
    ) {
        // Store plan content for display in conversation
        self.state.plan_content_display = Some(plan_content.clone());
        // Add plan as a message in the conversation
        self.state
            .messages
            .push(DisplayMessage::new(DisplayRole::Plan, plan_content.clone()));
        self.state.message_generation += 1;
        // Start the plan approval controller
        let _rx = self.plan_approval_controller.start(plan_content);
        // Store the oneshot sender to forward the decision back to the tool
        self.plan_approval_response_tx = Some(response_tx);
        self.state.dirty = true;
    }

    pub(super) fn handle_user_submit(&mut self, msg: &str) {
        // Consume pending plan request and prepend sentinel
        let forwarded = if self.state.pending_plan_request {
            self.state.pending_plan_request = false;
            self.state.mode = OperationMode::Normal;
            format!("\x00__PLAN_MODE__{}", msg)
        } else {
            msg.to_string()
        };
        // Forward to backend if channel is configured
        if let Some(ref tx) = self.user_message_tx {
            let _ = tx.send(forwarded);
            self.state.agent_active = true;
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_interrupt(&mut self) {
        // Cancel all active prompt controllers
        if self.ask_user_controller.active() {
            self.ask_user_controller.cancel();
            self.ask_user_response_tx.take();
        }
        if self.plan_approval_controller.active() {
            self.plan_approval_controller.cancel();
            self.plan_approval_response_tx.take();
        }
        if self.approval_controller.active() {
            self.approval_controller.cancel();
            self.approval_response_tx.take();
        }
        if self.state.agent_active {
            if let Some(ref token) = self.interrupt_token {
                token.request(); // Signals all layers simultaneously
            }
            self.state.agent_active = false;
            self.state.backgrounding_pending = false;
            self.state
                .pending_queue
                .retain(|item| !matches!(item, super::PendingItem::UserMessage(_)));
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_set_interrupt_token(&mut self, token: opendev_runtime::InterruptToken) {
        self.interrupt_token = Some(token);
    }

    pub(super) fn handle_mode_changed(&mut self, mode: String) {
        self.state.mode = match mode.as_str() {
            "plan" => OperationMode::Plan,
            _ => OperationMode::Normal,
        };
        self.state.dirty = true;
    }

    pub(super) fn handle_kill_task(&mut self, id: String) {
        let tm = self.task_manager.clone();
        let tx = self.event_tx.clone();
        let id_display = id.clone();
        tokio::spawn(async move {
            let mut mgr = tm.lock().await;
            let msg = if mgr.get_task(&id).is_some() {
                if mgr.kill_task(&id).await {
                    format!("Killed task '{id}'.")
                } else {
                    format!("Failed to kill task '{id}'.")
                }
            } else {
                format!("Task '{id}' not found.")
            };
            let _ = tx.send(AppEvent::AgentError(msg));
        });
        self.push_system_message(format!("Killing task '{id_display}'..."));
        self.state.dirty = true;
    }

    pub(super) fn handle_compaction_started(&mut self) {
        self.state.compaction_active = true;
        self.state.dirty = true;
    }

    pub(super) fn handle_compaction_finished(&mut self, success: bool, message: String) {
        self.state.compaction_active = false;
        if success {
            self.push_system_message(message);
        } else {
            self.push_system_message(format!("Compaction failed: {message}"));
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_turn_checkpointed(&mut self, undo_depth: usize) {
        self.state.undo_depth = undo_depth;
    }

    pub(super) fn handle_undo_result(&mut self, success: bool, message: String) {
        let level = if success {
            ToastLevel::Success
        } else {
            ToastLevel::Warning
        };
        self.state.toasts.push(Toast::new(message, level));
        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_redo_result(&mut self, success: bool, message: String) {
        let level = if success {
            ToastLevel::Success
        } else {
            ToastLevel::Warning
        };
        self.state.toasts.push(Toast::new(message, level));
        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_share_result(&mut self, path: String) {
        self.state
            .toasts
            .push(Toast::new(format!("Shared: {path}"), ToastLevel::Success));
        self.state.dirty = true;
    }

    pub(super) fn handle_file_changed(&mut self, paths: Vec<String>) {
        // Just mark dirty — file changes are informational
        let _ = paths;
        self.state.dirty = true;
    }

    pub(super) fn handle_session_title_updated(&mut self, title: String) {
        self.state.session_title = Some(title);
        self.state.dirty = true;
    }
}
