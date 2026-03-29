//! Subagent lifecycle event handlers.

use std::collections::HashMap;

use serde_json::Value;

use super::App;

impl App {
    pub(super) fn handle_subagent_started(
        &mut self,
        subagent_id: String,
        subagent_name: String,
        task: String,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
    ) {
        // SubagentDisplayState was eagerly created at ToolStarted time (same channel,
        // guaranteed ordering). Now we just fill in the subagent_id so subsequent
        // SubagentToolCall/Finished events (which use subagent_id) can find it.
        // Match by parent_tool_id first (reliable), then fall back to task text
        let found = self.state.active_subagents.iter_mut().find(|s| {
            s.subagent_id.is_empty()
                && (s.parent_tool_id.as_ref().is_some_and(|ptid| {
                    self.state.active_tools.iter().any(|t| {
                        t.id == *ptid
                            && t.name == "spawn_subagent"
                            && t.args.get("task").and_then(|v| v.as_str()) == Some(&task)
                    })
                }) || s.task == task)
        });
        if let Some(sa) = found {
            sa.subagent_id = subagent_id.clone();
            sa.name = subagent_name;
            if let Some(token) = cancel_token {
                self.state.subagent_cancel_tokens.insert(subagent_id, token);
            }
        } else {
            // Fallback: create if not found (e.g. ToolStarted was missed)
            let parent_tool_id = self
                .state
                .active_tools
                .iter()
                .find(|t| {
                    t.name == "spawn_subagent"
                        && t.args.get("task").and_then(|v| v.as_str()) == Some(&task)
                })
                .map(|t| t.id.clone());
            if parent_tool_id.is_some() {
                let mut sa = crate::widgets::nested_tool::SubagentDisplayState::new(
                    subagent_id.clone(),
                    subagent_name,
                    task,
                );
                sa.parent_tool_id = parent_tool_id;
                self.state.active_subagents.push(sa);
                if let Some(token) = cancel_token {
                    self.state.subagent_cancel_tokens.insert(subagent_id, token);
                }
            } else if let Some(bg_task_id) = self
                .state
                .bg_agent_manager
                .all_tasks()
                .iter()
                .find(|t| t.is_running() && t.pending_spawn_count > 0)
                .map(|t| t.task_id.clone())
            {
                // Route to background task
                self.state
                    .bg_subagent_map
                    .insert(subagent_id.clone(), bg_task_id.clone());
                self.state
                    .bg_agent_manager
                    .decrement_pending_spawn(&bg_task_id);
                self.state
                    .bg_agent_manager
                    .push_activity(&bg_task_id, format!("\u{25b8} {subagent_name}: {task}"));
                // Create display entry so task watcher shows tool-level detail
                let mut sa = crate::widgets::nested_tool::SubagentDisplayState::new(
                    subagent_id.clone(),
                    subagent_name,
                    task,
                );
                sa.backgrounded = true;
                self.state.active_subagents.push(sa);
                if let Some(token) = cancel_token {
                    self.state.subagent_cancel_tokens.insert(subagent_id, token);
                }
            }
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_subagent_tool_call(
        &mut self,
        subagent_id: String,
        tool_name: String,
        tool_id: String,
        args: HashMap<String, Value>,
    ) {
        if let Some(subagent) = self
            .state
            .active_subagents
            .iter_mut()
            .find(|s| s.subagent_id == subagent_id)
        {
            let is_bg = subagent.backgrounded;
            subagent.add_tool_call(tool_name.clone(), tool_id, args);
            // Also update bg_agent_manager for backgrounded subagents
            if is_bg && let Some(bg_task_id) = self.state.bg_subagent_map.get(&subagent_id).cloned()
            {
                let count = self
                    .state
                    .bg_agent_manager
                    .get_task(&bg_task_id)
                    .map(|t| t.tool_call_count + 1)
                    .unwrap_or(1);
                self.state
                    .bg_agent_manager
                    .update_progress(&bg_task_id, tool_name, count);
            }
        } else if let Some(bg_task_id) = self.state.bg_subagent_map.get(&subagent_id).cloned() {
            let count = self
                .state
                .bg_agent_manager
                .get_task(&bg_task_id)
                .map(|t| t.tool_call_count + 1)
                .unwrap_or(1);
            self.state
                .bg_agent_manager
                .update_progress(&bg_task_id, tool_name, count);
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_subagent_tool_complete(
        &mut self,
        subagent_id: String,
        tool_name: String,
        tool_id: String,
        success: bool,
    ) {
        if let Some(subagent) = self
            .state
            .active_subagents
            .iter_mut()
            .find(|s| s.subagent_id == subagent_id)
        {
            let is_bg = subagent.backgrounded;
            subagent.complete_tool_call(&tool_id, success);
            if is_bg && let Some(bg_task_id) = self.state.bg_subagent_map.get(&subagent_id).cloned()
            {
                let icon = if success { "\u{2713}" } else { "\u{2717}" };
                self.state
                    .bg_agent_manager
                    .push_activity(&bg_task_id, format!("  {icon} {tool_name}"));
            }
        } else if let Some(bg_task_id) = self.state.bg_subagent_map.get(&subagent_id).cloned() {
            let icon = if success { "\u{2713}" } else { "\u{2717}" };
            self.state
                .bg_agent_manager
                .push_activity(&bg_task_id, format!("  {icon} {tool_name}"));
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_subagent_finished(
        &mut self,
        subagent_id: String,
        success: bool,
        result_summary: String,
        tool_call_count: usize,
        shallow_warning: Option<String>,
    ) {
        if let Some(subagent) = self
            .state
            .active_subagents
            .iter_mut()
            .find(|s| s.subagent_id == subagent_id)
        {
            let is_bg = subagent.backgrounded;
            subagent.finish(
                success,
                result_summary.clone(),
                tool_call_count,
                shallow_warning,
            );
            if is_bg && let Some(bg_task_id) = self.state.bg_subagent_map.get(&subagent_id).cloned()
            {
                let status = if success { "completed" } else { "failed" };
                self.state.bg_agent_manager.push_activity(
                    &bg_task_id,
                    format!("  Subagent {status} · {tool_call_count} tools"),
                );
            }
        } else if let Some(bg_task_id) = self.state.bg_subagent_map.get(&subagent_id).cloned() {
            let status = if success { "completed" } else { "failed" };
            self.state.bg_agent_manager.push_activity(
                &bg_task_id,
                format!("  Subagent {status} · {tool_call_count} tools"),
            );
        }
        // Clean up per-subagent cancel token
        self.state.subagent_cancel_tokens.remove(&subagent_id);
        // Remove finished subagents after marking them
        // (keep them for one more render so the user sees the result)
        // Clamp focus after potential visibility change
        let total_visible = self.state.active_subagents.len()
            + self
                .state
                .bg_agent_manager
                .all_tasks()
                .iter()
                .filter(|t| !t.hidden)
                .count();
        if total_visible > 0 {
            self.state.task_watcher_focus = self.state.task_watcher_focus.min(total_visible - 1);
        } else {
            self.state.task_watcher_focus = 0;
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_subagent_token_update(
        &mut self,
        subagent_id: String,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        if let Some(subagent) = self
            .state
            .active_subagents
            .iter_mut()
            .find(|s| s.subagent_id == subagent_id)
        {
            subagent.add_tokens(input_tokens, output_tokens);
        }
        self.state.dirty = true;
    }
}
