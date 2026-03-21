//! Event dispatching: routes AppEvents to state mutations.

use crate::event::AppEvent;
use crate::widgets::{TodoDisplayItem, TodoDisplayStatus};

use super::{
    App, AutonomyLevel, DisplayMessage, DisplayRole, DisplayToolCall, OperationMode, ToolExecution,
    ToolState,
};

impl App {
    /// Batch all pending background results into a single injection message
    /// and send it silently to the LLM (not displayed in the conversation).
    pub(super) fn drain_background_results(&mut self) {
        if self.state.pending_bg_results.is_empty() {
            return;
        }
        let results = std::mem::take(&mut self.state.pending_bg_results);
        let mut parts = Vec::new();
        for r in &results {
            parts.push(format!(
                "[Background task [{id}] completed ({tools} tools)]\n\
                 Task: {query}\n\n\
                 {result}",
                id = r.task_id,
                tools = r.tool_call_count,
                query = r.query,
                result = r.result,
            ));
        }
        let msg = parts.join("\n\n");
        // Send directly to the LLM without displaying as a user message
        self.state.agent_active = true;
        self.state.message_generation += 1;
        let _ = self.event_tx.send(AppEvent::UserSubmit(msg));
        self.state.dirty = true;
    }

    pub(super) fn drain_next_pending_message(&mut self) {
        if let Some(queued_msg) = self.state.pending_messages.first().cloned() {
            self.state.pending_messages.remove(0);
            // Display the user message NOW (deferred from queue time)
            self.message_controller
                .handle_user_submit(&mut self.state, &queued_msg);
            self.state.message_generation += 1;
            // Send to agent backend
            self.state.agent_active = true;
            let _ = self.event_tx.send(AppEvent::UserSubmit(queued_msg));
            self.state.dirty = true;
        }
    }

    /// Dispatch an event to the appropriate handler.
    pub(super) fn handle_event(&mut self, event: AppEvent) {
        // Detect tab-switch return via timing gap: if >1s elapsed since last
        // user-interactive event, the user likely switched away and came back.
        // Force a full repaint to fix any screen corruption from the terminal
        // emulator (works even when FocusGained doesn't fire).
        let is_user_event = matches!(
            event,
            AppEvent::Key(_)
                | AppEvent::ScrollUp
                | AppEvent::ScrollDown
                | AppEvent::MouseDown { .. }
                | AppEvent::MouseDrag { .. }
                | AppEvent::MouseUp { .. }
        );
        if is_user_event {
            if let Some(last) = self.state.last_event_time
                && last.elapsed() > std::time::Duration::from_secs(1)
            {
                self.state.force_clear = true;
            }
            self.state.last_event_time = Some(std::time::Instant::now());
        }

        match event {
            AppEvent::Key(key) => {
                // Any keyboard input clears the selection
                if self.state.selection.range.is_some() {
                    self.state.selection.clear();
                }
                self.handle_key(key);
                self.state.dirty = true;
            }
            AppEvent::Resize(_, _) => {
                // Clear selection on resize (geometry changed)
                self.state.selection.clear();
                self.state.dirty = true;
            }
            AppEvent::FocusGained => {
                // Force full redraw when terminal regains focus to fix screen corruption.
                // Setting force_clear triggers terminal.clear() which resets ratatui's
                // internal diff buffer, ensuring every cell is repainted.
                self.state.force_clear = true;
                self.state.dirty = true;
            }
            AppEvent::ScrollUp => {
                let amount = self.accelerated_scroll(true);
                self.state.scroll_offset = self.state.scroll_offset.saturating_add(amount);
                self.state.user_scrolled = true;
                self.state.dirty = true;
            }
            AppEvent::ScrollDown => {
                if self.state.scroll_offset > 0 {
                    let amount = self.accelerated_scroll(false);
                    self.state.scroll_offset = self.state.scroll_offset.saturating_sub(amount);
                } else {
                    self.state.user_scrolled = false;
                }
                self.state.dirty = true;
            }
            AppEvent::MouseDown { col, row } => {
                self.handle_mouse_down(col, row);
                self.state.dirty = true;
            }
            AppEvent::MouseDrag { col, row } => {
                self.handle_mouse_drag(col, row);
                self.state.dirty = true;
            }
            AppEvent::MouseUp { col, row } => {
                self.handle_mouse_up(col, row);
                self.state.dirty = true;
            }
            AppEvent::Tick => {
                self.handle_tick();
                // Tick is dirty only when there are animations running
                if self.state.agent_active
                    || !self.state.active_tools.is_empty()
                    || !self.state.active_subagents.is_empty()
                    || self.state.task_progress.is_some()
                    || !self.state.welcome_panel.fade_complete
                    || self.state.task_watcher_open
                    || self.state.background_task_count > 0
                    || self.state.last_task_completion.is_some()
                    || self.state.backgrounded_task_info.is_some()
                    || !self.state.toasts.is_empty()
                    || self.state.leader_pending
                    || self.state.selection.active
                {
                    self.state.dirty = true;
                }
            }

            // Budget events
            AppEvent::BudgetExhausted {
                cost_usd,
                budget_usd,
            } => {
                self.state.agent_active = false;
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!(
                        "Session cost budget exhausted: ${:.4} spent of ${:.2} budget. \
                         Agent paused. Use /budget to adjust.",
                        cost_usd, budget_usd
                    ),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.dirty = true;
                self.state.message_generation += 1;
            }

            // File change summary events
            AppEvent::FileChangeSummary {
                files,
                additions,
                deletions,
            } => {
                if files > 0 {
                    self.state.file_changes = Some((files, additions, deletions));
                }
                self.state.dirty = true;
            }

            // Context usage events
            AppEvent::ContextUsage(pct) => {
                self.state.context_usage_pct = pct;
                self.state.dirty = true;
            }

            // Agent events
            AppEvent::AgentStarted => {
                self.state.agent_active = true;
                self.state.backgrounded_task_info = None;
                // Clear finished (non-backgrounded) subagents from previous query
                self.state.active_subagents.retain(|s| !s.finished || s.backgrounded);
                self.state.dirty = true;
            }
            AppEvent::AgentChunk(text) => {
                self.message_controller
                    .handle_agent_chunk(&mut self.state, &text);
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::AgentMessage(msg) => {
                self.message_controller
                    .handle_agent_message(&mut self.state, msg);
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::AgentFinished => {
                self.state.agent_active = false;
                self.state.backgrounding_pending = false;
                self.state.dirty = true;
                // Priority: user messages first, then background results
                if !self.state.pending_messages.is_empty() {
                    self.drain_next_pending_message();
                } else if !self.state.pending_bg_results.is_empty() {
                    self.drain_background_results();
                }
            }
            AppEvent::AgentError(err) => {
                self.state.agent_active = false;
                self.state.backgrounding_pending = false;
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("Error: {err}"),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.dirty = true;
                self.state.message_generation += 1;
                // Continue processing queued messages despite the error
                self.drain_next_pending_message();
            }

            // Reasoning events
            AppEvent::ReasoningBlockStart => {
                if let Some(last) = self.state.messages.last()
                    && last.role == DisplayRole::Reasoning
                    && !last.content.is_empty()
                {
                    self.state.messages.push(DisplayMessage {
                        role: DisplayRole::Reasoning,
                        content: String::new(),
                        tool_call: None,
                        collapsed: false,
                    });
                    self.state.dirty = true;
                    self.state.message_generation += 1;
                }
            }
            AppEvent::ReasoningContent(content) => {
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
                        collapsed: false,
                    });
                }
                self.state.dirty = true;
                self.state.message_generation += 1;
            }

            // Tool events
            AppEvent::ToolStarted {
                tool_id,
                tool_name,
                args,
            } => {
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
            AppEvent::ToolOutput { tool_id, output } => {
                if let Some(tool) = self.state.active_tools.iter_mut().find(|t| t.id == tool_id) {
                    tool.output_lines.push(output);
                }
                self.state.dirty = true;
            }
            AppEvent::ToolResult {
                tool_id,
                tool_name,
                output,
                success,
                args: result_args,
            } => {
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
                    let summary = crate::formatters::todo_formatter::summarize_todo_result(
                        &tool_name, &output,
                    );
                    (vec![summary], false)
                } else {
                    use crate::widgets::conversation::is_diff_tool;
                    let result_lines: Vec<String> =
                        output.lines().take(50).map(|l| l.to_string()).collect();
                    let lines = if result_lines.is_empty() && !output.is_empty() {
                        vec![output.clone()]
                    } else {
                        result_lines
                    };
                    use crate::formatters::tool_registry::{categorize_tool, ToolCategory};
                    let is_file_read =
                        categorize_tool(&tool_name) == ToolCategory::FileRead;
                    let collapse =
                        is_file_read || (lines.len() > 5 && !is_diff_tool(&tool_name));
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
                    // Find matching subagent by parent_tool_id (reliable), fall back to task text
                    let subagent_idx = self
                        .state
                        .active_subagents
                        .iter()
                        .position(|s| s.parent_tool_id.as_deref() == Some(&tool_id))
                        .or_else(|| {
                            let task_text =
                                arguments.get("task").and_then(|v| v.as_str()).unwrap_or("");
                            self.state
                                .active_subagents
                                .iter()
                                .position(|s| s.task == task_text)
                        });
                    let stats = if let Some(idx) = subagent_idx {
                        let subagent = self.state.active_subagents.remove(idx);
                        let mut tc = subagent.tool_call_count;
                        let tk = subagent.token_count;
                        let elapsed = subagent.started_at.elapsed();

                        // If bridge events haven't delivered stats yet, parse from
                        // the output header that SpawnSubagentTool embeds reliably.
                        if tc == 0
                            && let Some(line) = output.lines().next()
                            && let Some(rest) = line.strip_prefix("__subagent_stats__:")
                        {
                            for part in rest.split(',') {
                                if let Some(val) = part.strip_prefix("tc=") {
                                    tc = val.parse().unwrap_or(0);
                                }
                            }
                        }
                        (tc, tk, elapsed, subagent.success)
                    } else {
                        // No subagent found at all — try parsing from output header
                        let mut tc = 0usize;
                        if let Some(line) = output.lines().next()
                            && let Some(rest) = line.strip_prefix("__subagent_stats__:")
                        {
                            for part in rest.split(',') {
                                if let Some(val) = part.strip_prefix("tc=") {
                                    tc = val.parse().unwrap_or(0);
                                }
                            }
                        }
                        (tc, 0, std::time::Duration::ZERO, success)
                    };

                    let summary = if !success {
                        // Show first meaningful line of the output as error context
                        let error_hint = output
                            .lines()
                            .find(|l| {
                                !l.starts_with("__subagent_stats__:")
                                    && !l.starts_with("task_id:")
                                    && !l.trim().is_empty()
                            })
                            .unwrap_or("unknown error");
                        let truncated = if error_hint.len() > 120 {
                            format!("{}...", &error_hint[..120])
                        } else {
                            error_hint.to_string()
                        };
                        format!("Failed: {truncated}")
                    } else {
                        let elapsed_secs = stats.2.as_secs();
                        let elapsed_str = if elapsed_secs >= 60 {
                            format!("{}m {}s", elapsed_secs / 60, elapsed_secs % 60)
                        } else {
                            format!("{elapsed_secs}s")
                        };
                        let token_str = if stats.1 > 0 {
                            let k = stats.1 as f64 / 1000.0;
                            format!(" \u{00b7} {k:.1}k tokens")
                        } else {
                            String::new()
                        };
                        if stats.0 > 0 || stats.1 > 0 {
                            format!(
                                "Done ({} tool uses{} \u{00b7} {})",
                                stats.0, token_str, elapsed_str
                            )
                        } else {
                            "Done".to_string()
                        }
                    };
                    self.state.messages.push(DisplayMessage {
                        role: DisplayRole::Assistant,
                        content: String::new(),
                        tool_call: Some(DisplayToolCall {
                            name: tool_name.clone(),
                            arguments,
                            summary: None,
                            success,
                            collapsed: false,
                            result_lines: vec![summary],
                            nested_calls: Vec::new(),
                        }),
                        collapsed: false,
                    });
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
                    });
                }

                // Refresh todo panel from shared manager after any todo tool
                if is_todo_tool
                    && let Some(ref mgr) = self.state.todo_manager
                    && let Ok(mgr) = mgr.lock()
                {
                    self.state.todo_items = mgr
                        .all()
                        .iter()
                        .map(|item| TodoDisplayItem {
                            id: item.id,
                            title: item.title.clone(),
                            status: match item.status {
                                opendev_runtime::TodoStatus::Pending => TodoDisplayStatus::Pending,
                                opendev_runtime::TodoStatus::InProgress => {
                                    TodoDisplayStatus::InProgress
                                }
                                opendev_runtime::TodoStatus::Completed => {
                                    TodoDisplayStatus::Completed
                                }
                            },
                            active_form: if item.active_form.is_empty() {
                                None
                            } else {
                                Some(item.active_form.clone())
                            },
                        })
                        .collect();
                    if tool_name == "write_todos" && !self.state.todo_items.is_empty() {
                        self.state.todo_expanded = true;
                    }
                    if tool_name == "clear_todos" {
                        self.state.todo_items.clear();
                    }
                }

                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::ToolFinished { tool_id, success } => {
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
            AppEvent::ToolApprovalRequired {
                tool_id: _,
                tool_name: _,
                description,
            } => {
                // Legacy event without channel — activate controller without response_tx
                let wd = self.state.working_dir.clone();
                let _rx = self.approval_controller.start(description, wd);
                self.state.dirty = true;
            }
            AppEvent::ToolApprovalRequested {
                command,
                working_dir,
                response_tx,
            } => {
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
            AppEvent::AskUserRequested {
                question,
                options,
                default,
                response_tx,
            } => {
                self.ask_user_controller.start(question, options, default);
                self.ask_user_response_tx = Some(response_tx);
                self.state.dirty = true;
            }

            // Subagent events — use subagent_id for lookup to support parallel subagents
            AppEvent::SubagentStarted {
                subagent_id,
                subagent_name,
                task,
            } => {
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
                    sa.subagent_id = subagent_id;
                    sa.name = subagent_name;
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
                            subagent_id,
                            subagent_name,
                            task,
                        );
                        sa.parent_tool_id = parent_tool_id;
                        self.state.active_subagents.push(sa);
                    } else if let Some(bg_task_id) = self
                        .state
                        .bg_agent_manager
                        .all_tasks()
                        .iter()
                        .find(|t| {
                            t.is_running() && t.current_tool.as_deref() == Some("spawn_subagent")
                        })
                        .map(|t| t.task_id.clone())
                    {
                        // Route to background task
                        self.state
                            .bg_subagent_map
                            .insert(subagent_id.clone(), bg_task_id.clone());
                        self.state.bg_agent_manager.push_activity(
                            &bg_task_id,
                            format!("\u{25b8} {subagent_name}: {task}"),
                        );
                        // Create display entry so task watcher shows tool-level detail
                        let mut sa = crate::widgets::nested_tool::SubagentDisplayState::new(
                            subagent_id,
                            subagent_name,
                            task,
                        );
                        sa.backgrounded = true;
                        self.state.active_subagents.push(sa);
                    }
                }
                self.state.dirty = true;
            }
            AppEvent::SubagentToolCall {
                subagent_id,
                tool_name,
                tool_id,
                args,
                ..
            } => {
                if let Some(subagent) = self
                    .state
                    .active_subagents
                    .iter_mut()
                    .find(|s| s.subagent_id == subagent_id)
                {
                    let is_bg = subagent.backgrounded;
                    subagent.add_tool_call(tool_name.clone(), tool_id, args);
                    // Also update bg_agent_manager for backgrounded subagents
                    if is_bg
                        && let Some(bg_task_id) =
                            self.state.bg_subagent_map.get(&subagent_id).cloned()
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
                } else if let Some(bg_task_id) =
                    self.state.bg_subagent_map.get(&subagent_id).cloned()
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
                self.state.dirty = true;
            }
            AppEvent::SubagentToolComplete {
                subagent_id,
                tool_name,
                tool_id,
                success,
                ..
            } => {
                if let Some(subagent) = self
                    .state
                    .active_subagents
                    .iter_mut()
                    .find(|s| s.subagent_id == subagent_id)
                {
                    let is_bg = subagent.backgrounded;
                    subagent.complete_tool_call(&tool_id, success);
                    if is_bg
                        && let Some(bg_task_id) =
                            self.state.bg_subagent_map.get(&subagent_id).cloned()
                    {
                        let icon = if success { "\u{2713}" } else { "\u{2717}" };
                        self.state
                            .bg_agent_manager
                            .push_activity(&bg_task_id, format!("  {icon} {tool_name}"));
                    }
                } else if let Some(bg_task_id) =
                    self.state.bg_subagent_map.get(&subagent_id).cloned()
                {
                    let icon = if success { "\u{2713}" } else { "\u{2717}" };
                    self.state
                        .bg_agent_manager
                        .push_activity(&bg_task_id, format!("  {icon} {tool_name}"));
                }
                self.state.dirty = true;
            }
            AppEvent::SubagentFinished {
                subagent_id,
                success,
                result_summary,
                tool_call_count,
                shallow_warning,
                ..
            } => {
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
                    if is_bg
                        && let Some(bg_task_id) = self.state.bg_subagent_map.remove(&subagent_id)
                    {
                        let status = if success { "completed" } else { "failed" };
                        self.state.bg_agent_manager.push_activity(
                            &bg_task_id,
                            format!("  Subagent {status} ({tool_call_count} tools)"),
                        );
                    }
                } else if let Some(bg_task_id) = self.state.bg_subagent_map.remove(&subagent_id) {
                    let status = if success { "completed" } else { "failed" };
                    self.state.bg_agent_manager.push_activity(
                        &bg_task_id,
                        format!("  Subagent {status} ({tool_call_count} tools)"),
                    );
                }
                // Remove finished subagents after marking them
                // (keep them for one more render so the user sees the result)
                self.state.dirty = true;
            }

            AppEvent::SubagentTokenUpdate {
                subagent_id,
                input_tokens,
                output_tokens,
                ..
            } => {
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

            // Task progress events
            AppEvent::TaskProgressStarted { description } => {
                self.state.task_progress = Some(crate::widgets::progress::TaskProgress {
                    description,
                    elapsed_secs: 0,
                    token_display: None,
                    interrupted: false,
                    started_at: std::time::Instant::now(),
                });
                self.state.dirty = true;
            }
            AppEvent::TaskProgressFinished => {
                self.state.task_progress = None;
                self.state.dirty = true;
            }

            // UI events
            // Plan approval events
            AppEvent::PlanApprovalRequested {
                plan_content,
                response_tx,
            } => {
                // Store plan content for display in conversation
                self.state.plan_content_display = Some(plan_content.clone());
                // Add plan as a message in the conversation
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("── Plan ──\n{plan_content}"),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.message_generation += 1;
                // Start the plan approval controller
                let _rx = self.plan_approval_controller.start(plan_content);
                // Store the oneshot sender to forward the decision back to the tool
                self.plan_approval_response_tx = Some(response_tx);
                self.state.dirty = true;
            }

            AppEvent::UserSubmit(ref msg) => {
                // Forward to backend if channel is configured
                if let Some(ref tx) = self.user_message_tx {
                    let _ = tx.send(msg.clone());
                    self.state.agent_active = true;
                }
                self.state.dirty = true;
            }
            AppEvent::Interrupt => {
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
                    self.state.pending_messages.clear();
                }
                self.state.dirty = true;
            }
            AppEvent::SetInterruptToken(token) => {
                self.interrupt_token = Some(token);
            }
            AppEvent::AgentInterrupted => {
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
                // Show interrupt feedback in the conversation
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::Interrupt,
                    content: "Interrupted. What should I do instead?".to_string(),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::ModeChanged(mode) => {
                self.state.mode = match mode.as_str() {
                    "plan" => OperationMode::Plan,
                    _ => OperationMode::Normal,
                };
                self.state.dirty = true;
            }
            AppEvent::KillTask(id) => {
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
            AppEvent::CompactionStarted => {
                self.state.compaction_active = true;
                self.state.dirty = true;
            }
            AppEvent::CompactionFinished { success, message } => {
                self.state.compaction_active = false;
                if success {
                    self.push_system_message(message);
                } else {
                    self.push_system_message(format!("Compaction failed: {message}"));
                }
                self.state.dirty = true;
            }

            // Background agent events
            AppEvent::AgentBackgrounded {
                task_id,
                query_summary: _,
            } => {
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

                self.state.backgrounded_task_info =
                    Some((task_id.clone(), std::time::Instant::now()));
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::BackgroundNudge { content } => {
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::Assistant,
                    content,
                    tool_call: None,
                    collapsed: false,
                });
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::BackgroundAgentCompleted {
                task_id,
                success,
                result_summary,
                full_result,
                cost_usd,
                tool_call_count,
            } => {
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
                self.state.last_task_completion =
                    Some((task_id.clone(), std::time::Instant::now()));

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
                        .pending_bg_results
                        .push(super::PendingBackgroundResult {
                            task_id: task_id.clone(),
                            query,
                            result: full_result,
                            success,
                            tool_call_count: total_tools,
                            cost_usd,
                        });

                    // If idle, drain immediately
                    if !self.state.agent_active && self.state.pending_messages.is_empty() {
                        self.drain_background_results();
                    }
                }

                self.state.dirty = true;
            }
            AppEvent::BackgroundAgentProgress {
                task_id,
                tool_name,
                tool_count,
            } => {
                self.state
                    .bg_agent_manager
                    .update_progress(&task_id, tool_name, tool_count);
                self.state.dirty = true;
            }
            AppEvent::BackgroundAgentActivity { task_id, line } => {
                self.state.bg_agent_manager.push_activity(&task_id, line);
                self.state.dirty = true;
            }
            AppEvent::BackgroundAgentKilled { task_id } => {
                self.push_system_message(format!("Background agent [{task_id}] killed."));
                self.state.dirty = true;
            }
            AppEvent::SetBackgroundAgentToken {
                task_id,
                query,
                session_id,
                interrupt_token,
            } => {
                self.state
                    .bg_agent_manager
                    .add_task(task_id, query, session_id, interrupt_token);
                self.state.dirty = true;
            }

            // Undo/Redo/Share events
            AppEvent::SnapshotTaken { hash } => {
                self.state.undo_stack.push(hash);
                self.state.redo_stack.clear();
            }
            AppEvent::UndoResult { success, message } => {
                use crate::widgets::toast::{Toast, ToastLevel};
                let level = if success {
                    ToastLevel::Success
                } else {
                    ToastLevel::Warning
                };
                self.state.toasts.push(Toast::new(message, level));
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::RedoResult { success, message } => {
                use crate::widgets::toast::{Toast, ToastLevel};
                let level = if success {
                    ToastLevel::Success
                } else {
                    ToastLevel::Warning
                };
                self.state.toasts.push(Toast::new(message, level));
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::ShareResult { path } => {
                use crate::widgets::toast::{Toast, ToastLevel};
                self.state
                    .toasts
                    .push(Toast::new(format!("Shared: {path}"), ToastLevel::Success));
                self.state.dirty = true;
            }
            AppEvent::FileChanged { paths } => {
                // Just mark dirty — file changes are informational
                let _ = paths;
                self.state.dirty = true;
            }

            AppEvent::SessionTitleUpdated(title) => {
                self.state.session_title = Some(title);
                self.state.dirty = true;
            }

            AppEvent::Quit => {
                self.state.running = false;
                self.state.dirty = true;
            }

            // Passthrough for unhandled events
            _ => {}
        }
    }

    /// Handle mouse button press — start selection if in conversation area.
    fn handle_mouse_down(&mut self, col: u16, row: u16) {
        // Don't start selection during modal overlays
        if self.approval_controller.active()
            || self.ask_user_controller.active()
            || self.plan_approval_controller.active()
            || self
                .model_picker_controller
                .as_ref()
                .is_some_and(|p| p.active())
            || self.state.task_watcher_open
            || self.state.debug_panel_open
        {
            return;
        }

        if self.state.selection.is_in_conversation_area(col, row) {
            self.state.selection.start(col, row);
        } else {
            self.state.selection.clear();
        }
    }

    /// Handle mouse drag — extend selection, set auto-scroll direction.
    fn handle_mouse_drag(&mut self, col: u16, row: u16) {
        if !self.state.selection.active {
            return;
        }
        self.state.selection.extend(col, row);
    }

    /// Handle mouse button release — finalize selection and copy to clipboard.
    fn handle_mouse_up(&mut self, col: u16, row: u16) {
        if !self.state.selection.active {
            return;
        }

        // Update cursor position one last time
        self.state.selection.extend(col, row);

        if self.state.selection.finalize() {
            // Extract and copy selected text
            if let Some(text) = self.extract_selected_text() {
                self.copy_to_clipboard(&text);
            }
        }
    }

    /// Extract plain text from the selected range of cached lines.
    fn extract_selected_text(&self) -> Option<String> {
        let range = self.state.selection.range?;
        let (start, end) = range.ordered();
        let lines = &self.state.cached_lines;

        if lines.is_empty() || start.line_index >= lines.len() {
            return None;
        }

        let end_line = end.line_index.min(lines.len().saturating_sub(1));
        let mut result = String::new();

        for (i, line) in lines[start.line_index..=end_line].iter().enumerate() {
            let line_idx = start.line_index + i;
            if i > 0 {
                result.push('\n');
            }

            // Collect the full text of this line from spans
            let full_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

            let col_start = if line_idx == start.line_index {
                start.char_offset
            } else {
                0
            };
            let col_end = if line_idx == end.line_index {
                end.char_offset
            } else {
                full_text.len()
            };

            // Clamp to actual line length (char boundary safe)
            let clamped_start = col_start.min(full_text.len());
            let clamped_end = col_end.min(full_text.len());
            if clamped_start < clamped_end {
                // Find char boundaries
                let byte_start = full_text
                    .char_indices()
                    .nth(clamped_start)
                    .map(|(i, _)| i)
                    .unwrap_or(full_text.len());
                let byte_end = full_text
                    .char_indices()
                    .nth(clamped_end)
                    .map(|(i, _)| i)
                    .unwrap_or(full_text.len());
                result.push_str(&full_text[byte_start..byte_end]);
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Copy text to the system clipboard.
    fn copy_to_clipboard(&mut self, text: &str) {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                if let Err(e) = clipboard.set_text(text) {
                    tracing::warn!("Failed to copy to clipboard: {e}");
                }
            }
            Err(e) => {
                tracing::warn!("Failed to access clipboard: {e}");
            }
        }
    }
}
