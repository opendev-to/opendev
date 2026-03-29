//! Background agent event handlers.

use super::{App, DisplayMessage, DisplayRole, DisplayToolCall};

impl App {
    pub(super) fn handle_agent_backgrounded(&mut self, task_id: String) {
        self.state.backgrounding_pending = false;

        // Close out any active tools with "Sent to background" result
        for tool in std::mem::take(&mut self.state.active_tools) {
            self.state.messages.push(DisplayMessage {
                role: DisplayRole::Assistant,
                content: String::new(),
                tool_call: Some(DisplayToolCall {
                    name: tool.name.clone(),
                    arguments: tool.args.clone(),
                    summary: None,
                    success: true,
                    collapsed: false,
                    result_lines: vec!["Sent to background".to_string()],
                    nested_calls: Vec::new(),
                }),
                collapsed: false,
                thinking_started_at: None,
                thinking_duration_secs: None,
            });
        }
        // Mark surviving (finished) subagents as backgrounded.
        // Non-finished subagents are removed — their foreground processes
        // were cancelled by the background interrupt. The bg runtime will
        // re-spawn them with new IDs and SubagentStarted will create fresh
        // display entries.
        for sa in &mut self.state.active_subagents {
            sa.backgrounded = true;
        }
        self.state.active_subagents.retain(|s| s.finished);
        self.state.task_progress = None;

        self.state.backgrounded_task_info = Some((task_id.clone(), std::time::Instant::now()));
        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_background_nudge(&mut self, content: String) {
        self.state
            .messages
            .push(DisplayMessage::new(DisplayRole::Assistant, content));
        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_background_agent_completed(
        &mut self,
        task_id: String,
        success: bool,
        result_summary: String,
        full_result: String,
        cost_usd: f64,
        tool_call_count: usize,
    ) {
        // Check if the task was killed before queuing results
        let was_killed = self
            .state
            .bg_agent_manager
            .get_task(&task_id)
            .is_some_and(|t| {
                t.state == crate::managers::background_agents::BackgroundAgentState::Killed
            });

        // Use the higher tool count — bg_agent_manager tracks subagent
        // tools via update_progress, while the callback only counts
        // top-level tool calls.
        let tracked_count = self
            .state
            .bg_agent_manager
            .get_task(&task_id)
            .map(|t| t.tool_call_count)
            .unwrap_or(0);
        let total_tools = tracked_count.max(tool_call_count);

        self.state.bg_agent_manager.mark_completed(
            &task_id,
            success,
            result_summary.clone(),
            total_tools,
            cost_usd,
        );
        self.state.last_task_completion = Some((task_id.clone(), std::time::Instant::now()));

        // When a bg task was killed, mark all its child subagents as killed too
        if was_killed {
            let killed_subagent_ids: Vec<String> = self
                .state
                .bg_subagent_map
                .iter()
                .filter(|(_, bg_id)| *bg_id == &task_id)
                .map(|(sa_id, _)| sa_id.clone())
                .collect();
            for sa_id in &killed_subagent_ids {
                if let Some(sa) = self
                    .state
                    .active_subagents
                    .iter_mut()
                    .find(|s| s.subagent_id == *sa_id)
                    && !sa.finished
                {
                    sa.finish(false, "Killed".to_string(), sa.tool_call_count, None);
                }
                self.state.bg_subagent_map.remove(sa_id);
                self.state.subagent_cancel_tokens.remove(sa_id);
            }
        }

        // Clean up child subagents belonging to this completed task
        let child_sa_ids: Vec<String> = self
            .state
            .bg_subagent_map
            .iter()
            .filter(|(_, bg_id)| *bg_id == &task_id)
            .map(|(sa_id, _)| sa_id.clone())
            .collect();
        for sa_id in &child_sa_ids {
            self.state.bg_subagent_map.remove(sa_id);
        }
        // Remove finished backgrounded subagents belonging to this completed task
        self.state
            .active_subagents
            .retain(|s| !(s.backgrounded && s.finished && child_sa_ids.contains(&s.subagent_id)));

        // Clear backgrounded_task_info if it matches this task
        if let Some((ref info_id, _)) = self.state.backgrounded_task_info
            && info_id == &task_id
        {
            self.state.backgrounded_task_info = None;
        }

        // Queue successful, non-killed results for injection
        if success && !was_killed {
            let query = self
                .state
                .bg_agent_manager
                .get_task(&task_id)
                .map(|t| t.query.clone())
                .unwrap_or_default();
            self.state
                .pending_queue
                .push_back(super::PendingItem::BackgroundResult {
                    task_id: task_id.clone(),
                    query,
                    result: full_result,
                    success,
                    tool_call_count: total_tools,
                    cost_usd,
                });

            // If idle, drain immediately
            if !self.state.agent_active {
                self.drain_next_pending();
            }
        }

        self.state.dirty = true;
    }

    pub(super) fn handle_background_agent_progress(
        &mut self,
        task_id: String,
        tool_name: String,
        tool_count: usize,
    ) {
        if tool_name == "spawn_subagent" {
            self.state
                .bg_agent_manager
                .increment_pending_spawn(&task_id);
        }
        self.state
            .bg_agent_manager
            .update_progress(&task_id, tool_name, tool_count);
        self.state.dirty = true;
    }

    pub(super) fn handle_background_agent_activity(&mut self, task_id: String, line: String) {
        self.state.bg_agent_manager.push_activity(&task_id, line);
        self.state.dirty = true;
    }

    pub(super) fn handle_background_agent_killed(&mut self, task_id: String) {
        self.push_system_message(format!("Background agent [{task_id}] killed."));
        self.state.dirty = true;
    }

    pub(super) fn handle_set_background_agent_token(
        &mut self,
        task_id: String,
        query: String,
        session_id: String,
        interrupt_token: opendev_runtime::InterruptToken,
    ) {
        self.state
            .bg_agent_manager
            .add_task(task_id, query, session_id, interrupt_token);
        self.state.dirty = true;
    }
}
