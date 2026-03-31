//! Agent lifecycle event handlers.

use opendev_models::message::ChatMessage;

use super::event_dispatch::map_todo_items;
use super::{App, DisplayMessage, DisplayRole};

impl App {
    pub(super) fn handle_agent_started(&mut self) {
        self.state.agent_active = true;
        self.state.backgrounded_task_info = None;
        self.state.turn_token_count = 0;
        self.state.turn_started_at = Some(std::time::Instant::now());
        // Clear finished (non-backgrounded) subagents from previous query
        self.state
            .active_subagents
            .retain(|s| !s.finished || s.backgrounded);
        // Auto-resume next pending todo when agent restarts after interrupt
        // (reset_stuck_todos reverted InProgress→Pending; restore it now)
        if let Some(ref mgr) = self.state.todo_manager
            && let Ok(mut mgr) = mgr.lock()
            && mgr.has_todos()
            && !mgr.all_completed()
            && mgr.in_progress_count() == 0
        {
            if let Some(next) = mgr.next_pending() {
                let id = next.id;
                mgr.start(id);
            }
            self.state.todo_items = map_todo_items(&mgr);
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_agent_chunk(&mut self, text: String) {
        self.finalize_active_thinking();
        self.message_controller
            .handle_agent_chunk(&mut self.state, &text);
        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_agent_message(&mut self, msg: ChatMessage) {
        self.finalize_active_thinking();
        self.message_controller
            .handle_agent_message(&mut self.state, msg);
        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_agent_finished(&mut self) {
        self.finalize_active_thinking();
        self.state.agent_active = false;
        self.state.last_token_at = None;
        self.state.turn_started_at = None;
        self.state.backgrounding_pending = false;
        self.state.dirty = true;
        self.drain_next_pending();
    }

    pub(super) fn handle_agent_error(&mut self, err: String) {
        self.finalize_active_thinking();
        self.state.agent_active = false;
        self.state.backgrounding_pending = false;
        self.state.messages.push(DisplayMessage::new(
            DisplayRole::System,
            format!("Error: {err}"),
        ));
        self.state.dirty = true;
        self.state.message_generation += 1;
        // Continue processing queued items despite the error
        self.drain_next_pending();
    }

    pub(super) fn handle_reasoning_block_start(&mut self) {
        // Insert separator between multiple thinking blocks
        if let Some(last) = self.state.messages.last_mut()
            && last.role == DisplayRole::Reasoning
            && !last.content.is_empty()
        {
            last.content.push_str("\n\n");
            self.state.dirty = true;
            self.state.message_generation += 1;
        }
    }

    pub(super) fn handle_reasoning_content(&mut self, content: String) {
        // Append to previous reasoning message in this turn (streaming sends deltas)
        if let Some(last) = self.state.messages.last_mut()
            && last.role == DisplayRole::Reasoning
        {
            last.content.push_str(&content);
        } else {
            self.state.messages.push(DisplayMessage {
                role: DisplayRole::Reasoning,
                content,
                tool_call: None,
                collapsed: !self.state.thinking_expanded,
                thinking_started_at: Some(std::time::Instant::now()),
                thinking_duration_secs: None,
                thinking_finalized_at: None,
            });
        }
        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_agent_interrupted(&mut self) {
        self.finalize_active_thinking();
        self.state.agent_active = false;
        self.state.backgrounding_pending = false;
        self.state.task_progress = None;
        // Clear active tools
        self.state.active_tools.clear();
        // Mark any active subagents as interrupted and clear
        for subagent in &mut self.state.active_subagents {
            if !subagent.finished {
                subagent.finish(
                    false,
                    "Interrupted".to_string(),
                    subagent.tool_call_count,
                    None,
                );
            }
        }
        self.state.active_subagents.clear();
        // Re-sync TUI todo display after interrupt
        // (InProgress items were reset to Pending by reset_stuck_todos)
        self.sync_todo_display();
        // Show interrupt feedback in the conversation
        self.state.messages.push(DisplayMessage::new(
            DisplayRole::Interrupt,
            "Interrupted. What should I do instead?",
        ));
        self.state.dirty = true;
        self.state.message_generation += 1;
    }
}
