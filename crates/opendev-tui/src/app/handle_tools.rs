//! Tool execution event handlers.

use std::collections::HashMap;

use serde_json::Value;

use super::event_dispatch::map_todo_items;
use super::{
    App, AutonomyLevel, DisplayMessage, DisplayRole, DisplayToolCall, ToolExecution, ToolState,
};

impl App {
    pub(super) fn handle_tool_started(
        &mut self,
        tool_id: String,
        tool_name: String,
        args: HashMap<String, Value>,
    ) {
        self.finalize_active_thinking();
        // For spawn_subagent, eagerly create SubagentDisplayState now.
        // This avoids the race where SubagentStarted (forwarded by the bridge task)
        // arrives after ToolResult (sent directly), causing stats to be lost.
        if tool_name == "spawn_subagent" {
            // If all existing subagents are finished, this is a new batch — clear stale entries
            let all_finished = !self.state.active_subagents.is_empty()
                && self.state.active_subagents.iter().all(|s| s.finished);
            if all_finished {
                self.state.active_subagents.retain(|s| s.backgrounded);
            }

            let agent_name = args
                .get("agent_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Agent")
                .to_string();
            let task = args
                .get("task")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut sa = crate::widgets::nested_tool::SubagentDisplayState::new(
                String::new(), // subagent_id filled in by SubagentStarted later
                agent_name,
                task,
            );
            sa.parent_tool_id = Some(tool_id.clone());
            sa.description = args
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from);
            self.state.active_subagents.push(sa);
        }

        self.state.active_tools.push(ToolExecution {
            id: tool_id,
            name: tool_name,
            output_lines: Vec::new(),
            state: ToolState::Running,
            elapsed_secs: 0,
            started_at: std::time::Instant::now(),
            tick_count: 0,
            parent_id: None,
            depth: 0,
            args,
        });
        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_tool_output(&mut self, tool_id: String, output: String) {
        if let Some(tool) = self.state.active_tools.iter_mut().find(|t| t.id == tool_id) {
            tool.output_lines.push(output);
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_tool_result(
        &mut self,
        tool_id: String,
        tool_name: String,
        output: String,
        success: bool,
        result_args: HashMap<String, Value>,
    ) {
        // Look up stored args from the ToolStarted event, fall back to result args
        let arguments = self
            .state
            .active_tools
            .iter()
            .find(|t| t.id == tool_id)
            .map(|t| t.args.clone())
            .unwrap_or(result_args);

        // Check if this is a todo tool for special handling
        let is_todo_tool = matches!(
            tool_name.as_str(),
            "write_todos" | "update_todo" | "complete_todo" | "list_todos" | "clear_todos"
        );

        let (display_lines, collapsed) = if tool_name == "ask_user" {
            // Format as "· question → answer"
            let question = arguments
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("question");
            let answer = output.strip_prefix("User answered: ").unwrap_or(&output);
            (vec![format!("· {question} → {answer}")], false)
        } else if is_todo_tool {
            let summary =
                crate::formatters::todo_formatter::summarize_todo_result(&tool_name, &output);
            (vec![summary], false)
        } else if tool_name == "present_plan" {
            // Plan content is already displayed via PlanApprovalRequested → DisplayRole::Plan.
            // Show brief approval confirmation instead of full plan content.
            let step_count = output
                .split_once(" steps)")
                .and_then(|(before, _)| before.rsplit(", ").next())
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            if step_count > 0 {
                (
                    vec![format!("Plan approved · {step_count} todos created")],
                    false,
                )
            } else {
                (vec!["Plan approved".to_string()], false)
            }
        } else {
            use crate::widgets::conversation::is_diff_tool;
            let result_lines: Vec<String> =
                output.lines().take(50).map(|l| l.to_string()).collect();
            let lines = if result_lines.is_empty() && !output.is_empty() {
                vec![output.clone()]
            } else {
                result_lines
            };
            use crate::formatters::tool_registry::{ToolCategory, categorize_tool};
            let is_file_read = categorize_tool(&tool_name) == ToolCategory::FileRead;
            let collapse = is_file_read || (lines.len() > 5 && !is_diff_tool(&tool_name));
            (lines, collapse)
        };

        // For spawn_subagent, extract stats from tracked subagent state
        // Each subagent is treated independently — no grouping
        // Skip if the subagent was backgrounded — "Sent to background" message
        // was already created by AgentBackgrounded handler.
        if tool_name == "spawn_subagent"
            && self
                .state
                .active_subagents
                .iter()
                .any(|s| s.backgrounded && s.parent_tool_id.as_deref() == Some(&tool_id))
        {
            // Remove the backgrounded subagent from tracking
            self.state.active_tools.retain(|t| t.id != tool_id);
            self.state.dirty = true;
            return;
        }
        if tool_name == "spawn_subagent" {
            // Remove matching subagent from active list, creating a persistent summary
            let subagent_idx = self
                .state
                .active_subagents
                .iter()
                .position(|s| s.parent_tool_id.as_deref() == Some(&tool_id))
                .or_else(|| {
                    let task_text = arguments.get("task").and_then(|v| v.as_str()).unwrap_or("");
                    self.state
                        .active_subagents
                        .iter()
                        .position(|s| s.task == task_text)
                });
            if let Some(idx) = subagent_idx {
                let removed = self.state.active_subagents.remove(idx);
                self.state.bg_subagent_map.remove(&removed.subagent_id);

                // Create persistent completion summary in conversation
                let summary_line = removed.completion_summary();
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::Assistant,
                    content: String::new(),
                    tool_call: Some(DisplayToolCall {
                        name: tool_name.clone(),
                        arguments,
                        summary: None,
                        success,
                        collapsed: false,
                        result_lines: vec![summary_line],
                        nested_calls: Vec::new(),
                    }),
                    collapsed: false,
                    thinking_started_at: None,
                    thinking_duration_secs: None,
                });
            }
        } else if !display_lines.is_empty() {
            self.state.messages.push(DisplayMessage {
                role: DisplayRole::Assistant,
                content: String::new(),
                tool_call: Some(DisplayToolCall {
                    name: tool_name.clone(),
                    arguments,
                    summary: None,
                    success,
                    collapsed,
                    result_lines: display_lines,
                    nested_calls: Vec::new(),
                }),
                collapsed: false,
                thinking_started_at: None,
                thinking_duration_secs: None,
            });
        }

        // Refresh todo panel from shared manager after any todo tool or present_plan
        if (is_todo_tool || tool_name == "present_plan")
            && let Some(ref mgr) = self.state.todo_manager
            && let Ok(mgr) = mgr.lock()
        {
            self.state.todo_items = map_todo_items(&mgr);
            if (tool_name == "write_todos" || tool_name == "present_plan")
                && !self.state.todo_items.is_empty()
            {
                self.state.todo_expanded = true;
            }
            if tool_name == "clear_todos" {
                self.state.todo_items.clear();
            }
        }

        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_tool_finished(&mut self, tool_id: String, success: bool) {
        if let Some(tool) = self.state.active_tools.iter_mut().find(|t| t.id == tool_id) {
            tool.state = if success {
                ToolState::Completed
            } else {
                ToolState::Error
            };
        }
        // Remove finished tools after a brief display period
        self.state.active_tools.retain(|t| !t.is_finished());
        self.state.dirty = true;
        self.state.message_generation += 1;
    }

    pub(super) fn handle_tool_approval_required(&mut self, description: String) {
        // Legacy event without channel — activate controller without response_tx
        let wd = self.state.working_dir.clone();
        let _rx = self.approval_controller.start(description, wd);
        self.state.dirty = true;
    }

    pub(super) fn handle_tool_approval_requested(
        &mut self,
        command: String,
        working_dir: String,
        response_tx: tokio::sync::oneshot::Sender<opendev_runtime::ToolApprovalDecision>,
    ) {
        // Check autonomy level to decide whether to auto-approve.
        let auto_approve = match self.state.autonomy {
            AutonomyLevel::Auto => true,
            AutonomyLevel::SemiAuto => opendev_runtime::is_safe_command(&command),
            AutonomyLevel::Manual => false,
        };

        if auto_approve {
            let _ = response_tx.send(opendev_runtime::ToolApprovalDecision {
                approved: true,
                choice: "yes".to_string(),
                command,
            });
        } else {
            let _rx = self.approval_controller.start(command, working_dir);
            self.approval_response_tx = Some(response_tx);
        }
        self.state.dirty = true;
    }

    pub(super) fn handle_ask_user_requested(
        &mut self,
        question: String,
        options: Vec<String>,
        default: Option<String>,
        response_tx: tokio::sync::oneshot::Sender<String>,
    ) {
        self.ask_user_controller.start(question, options, default);
        self.ask_user_response_tx = Some(response_tx);
        self.state.dirty = true;
    }
}
