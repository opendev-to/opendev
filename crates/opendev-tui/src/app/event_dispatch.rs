//! Event dispatching: routes AppEvents to state mutations.

use crate::event::AppEvent;
use crate::widgets::{TodoDisplayItem, TodoDisplayStatus};

use super::{App, DisplayRole};

/// Map TodoManager items to TUI display items.
pub(super) fn map_todo_items(mgr: &opendev_runtime::TodoManager) -> Vec<TodoDisplayItem> {
    mgr.all()
        .iter()
        .map(|item| TodoDisplayItem {
            id: item.id,
            title: item.title.clone(),
            status: match item.status {
                opendev_runtime::TodoStatus::Pending => TodoDisplayStatus::Pending,
                opendev_runtime::TodoStatus::InProgress => TodoDisplayStatus::InProgress,
                opendev_runtime::TodoStatus::Completed => TodoDisplayStatus::Completed,
            },
            active_form: if item.active_form.is_empty() {
                None
            } else {
                Some(item.active_form.clone())
            },
        })
        .collect()
}

impl App {
    /// Synchronize TUI todo display items from the shared TodoManager.
    pub(super) fn sync_todo_display(&mut self) {
        if let Some(ref mgr) = self.state.todo_manager
            && let Ok(mgr) = mgr.lock()
        {
            self.state.todo_items = map_todo_items(&mgr);
        }
    }
    /// Record that the agent produced output (for stall detection in the spinner).
    /// Optionally increments the turn token estimate.
    pub(super) fn touch_last_token(&mut self) {
        self.state.last_token_at = Some(std::time::Instant::now());
    }

    /// Estimate and accumulate tokens from a streaming text chunk.
    pub(super) fn accumulate_turn_tokens(&mut self, char_count: usize) {
        // Rough estimate: ~4 chars per token
        self.state.turn_token_count += (char_count / 4).max(1) as u64;
    }

    /// Finalize the most recent unfinalized thinking block by freezing its duration.
    /// Called when a non-reasoning event arrives (AgentChunk, ToolStarted, etc.).
    pub(super) fn finalize_active_thinking(&mut self) {
        if let Some(msg) = self
            .state
            .messages
            .iter_mut()
            .rev()
            .find(|m| m.role == DisplayRole::Reasoning && m.thinking_duration_secs.is_none())
        {
            if let Some(started) = msg.thinking_started_at {
                msg.thinking_duration_secs = Some(started.elapsed().as_secs());
            } else {
                // History replay or missing start time
                msg.thinking_duration_secs = Some(0);
            }
            msg.thinking_finalized_at = Some(std::time::Instant::now());
            self.state.message_generation += 1;
        }
    }

    /// Drain the next pending item from the unified queue.
    /// User messages are sent one at a time. Consecutive background results are batched.
    pub(super) fn drain_next_pending(&mut self) {
        if self.state.pending_queue.is_empty() {
            return;
        }
        match self.state.pending_queue.front() {
            Some(super::PendingItem::UserMessage(_)) => {
                if let Some(super::PendingItem::UserMessage(msg)) =
                    self.state.pending_queue.pop_front()
                {
                    // Display the user message NOW (deferred from queue time)
                    self.message_controller
                        .handle_user_submit(&mut self.state, &msg);
                    self.state.message_generation += 1;
                    self.state.agent_active = true;
                    let _ = self.event_tx.send(AppEvent::UserSubmit(msg));
                    self.state.dirty = true;
                }
            }
            Some(super::PendingItem::BackgroundResult { .. }) => {
                // Process one background result at a time so the foreground agent
                // reasons about each task independently (no cross-group pollution).
                if let Some(super::PendingItem::BackgroundResult {
                    task_id,
                    query,
                    result,
                    tool_call_count,
                    ..
                }) = self.state.pending_queue.pop_front()
                {
                    // Push a display-layer tool call boundary so that
                    // handle_agent_chunk sees tool_call.is_some() on the last
                    // message and creates a NEW assistant message for the LLM's
                    // synthesized response — matching the foreground subagent flow.
                    let mut arguments = std::collections::HashMap::new();
                    arguments.insert("task_id".to_string(), serde_json::json!(&task_id));
                    self.state.messages.push(super::DisplayMessage {
                        role: super::DisplayRole::Assistant,
                        content: String::new(),
                        tool_call: Some(super::DisplayToolCall {
                            name: "get_background_result".to_string(),
                            arguments,
                            summary: None,
                            success: true,
                            collapsed: false,
                            result_lines: vec![format!(
                                "Background task [{task_id}] completed ({tool_call_count} tools) for: {query}"
                            )],
                            nested_calls: Vec::new(),
                            error_text: None,
                        }),
                        collapsed: false,
                        thinking_started_at: None,
                        thinking_duration_secs: None,
                        thinking_finalized_at: None,
                    });

                    // Send to the tui_runner as a tool-result injection (not a
                    // user message) so the LLM sees it as a tool call outcome.
                    let payload = serde_json::json!({
                        "task_id": task_id,
                        "query": query,
                        "result": result,
                        "tool_call_count": tool_call_count,
                    });
                    let sentinel = format!("\x00__BG_RESULT__{}", payload);
                    if let Some(ref tx) = self.user_message_tx {
                        let _ = tx.send(sentinel);
                    }
                    self.state.agent_active = true;
                    self.state.message_generation += 1;
                    self.state.dirty = true;
                }
            }
            None => {}
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
            } => self.handle_budget_exhausted(cost_usd, budget_usd),

            // File change summary events
            AppEvent::FileChangeSummary {
                files,
                additions,
                deletions,
            } => self.handle_file_change_summary(files, additions, deletions),

            // Context usage events
            AppEvent::ContextUsage(pct) => self.handle_context_usage(pct),

            // Agent events
            AppEvent::AgentStarted => self.handle_agent_started(),
            AppEvent::AgentChunk(text) => {
                self.touch_last_token();
                self.accumulate_turn_tokens(text.len());
                self.handle_agent_chunk(text);
            }
            AppEvent::AgentMessage(msg) => self.handle_agent_message(msg),
            AppEvent::AgentFinished => self.handle_agent_finished(),
            AppEvent::AgentError(err) => self.handle_agent_error(err),

            // Reasoning events
            AppEvent::ReasoningBlockStart => self.handle_reasoning_block_start(),
            AppEvent::ReasoningContent(content) => {
                self.touch_last_token();
                self.accumulate_turn_tokens(content.len());
                self.handle_reasoning_content(content);
            }

            // Tool events
            AppEvent::ToolStarted {
                tool_id,
                tool_name,
                args,
            } => {
                self.touch_last_token();
                self.handle_tool_started(tool_id, tool_name, args);
            }
            AppEvent::ToolOutput { tool_id, output } => self.handle_tool_output(tool_id, output),
            AppEvent::ToolResult {
                tool_id,
                tool_name,
                output,
                success,
                args: result_args,
            } => {
                self.touch_last_token();
                self.handle_tool_result(tool_id, tool_name, output, success, result_args);
            }
            AppEvent::ToolFinished { tool_id, success } => {
                self.handle_tool_finished(tool_id, success)
            }
            AppEvent::ToolApprovalRequired {
                tool_id: _,
                tool_name: _,
                description,
            } => self.handle_tool_approval_required(description),
            AppEvent::ToolApprovalRequested {
                command,
                working_dir,
                response_tx,
            } => self.handle_tool_approval_requested(command, working_dir, response_tx),
            AppEvent::AskUserRequested {
                question,
                options,
                default,
                response_tx,
            } => self.handle_ask_user_requested(question, options, default, response_tx),

            // Subagent events
            AppEvent::SubagentStarted {
                subagent_id,
                subagent_name,
                task,
                cancel_token,
            } => self.handle_subagent_started(subagent_id, subagent_name, task, cancel_token),
            AppEvent::SubagentToolCall {
                subagent_id,
                tool_name,
                tool_id,
                args,
                ..
            } => {
                self.touch_last_token();
                self.handle_subagent_tool_call(subagent_id, tool_name, tool_id, args);
            }
            AppEvent::SubagentToolComplete {
                subagent_id,
                tool_name,
                tool_id,
                success,
                ..
            } => self.handle_subagent_tool_complete(subagent_id, tool_name, tool_id, success),
            AppEvent::SubagentFinished {
                subagent_id,
                success,
                result_summary,
                tool_call_count,
                shallow_warning,
                ..
            } => self.handle_subagent_finished(
                subagent_id,
                success,
                result_summary,
                tool_call_count,
                shallow_warning,
            ),
            AppEvent::SubagentTokenUpdate {
                subagent_id,
                input_tokens,
                output_tokens,
                ..
            } => self.handle_subagent_token_update(subagent_id, input_tokens, output_tokens),

            // Task progress events
            AppEvent::TaskProgressStarted { description } => {
                self.touch_last_token();
                self.handle_task_progress_started(description);
            }
            AppEvent::TaskProgressFinished => self.handle_task_progress_finished(),

            // Plan approval events
            AppEvent::PlanApprovalRequested {
                plan_content,
                response_tx,
            } => self.handle_plan_approval_requested(plan_content, response_tx),

            AppEvent::UserSubmit(ref msg) => self.handle_user_submit(msg),
            AppEvent::Interrupt => self.handle_interrupt(),
            AppEvent::SetInterruptToken(token) => self.handle_set_interrupt_token(token),
            AppEvent::AgentInterrupted => self.handle_agent_interrupted(),
            AppEvent::ModeChanged(mode) => self.handle_mode_changed(mode),
            AppEvent::KillTask(id) => self.handle_kill_task(id),
            AppEvent::CompactionStarted => self.handle_compaction_started(),
            AppEvent::CompactionFinished { success, message } => {
                self.handle_compaction_finished(success, message)
            }

            // Background agent events
            AppEvent::AgentBackgrounded {
                task_id,
                query_summary: _,
            } => self.handle_agent_backgrounded(task_id),
            AppEvent::BackgroundNudge { content } => self.handle_background_nudge(content),
            AppEvent::BackgroundAgentCompleted {
                task_id,
                success,
                result_summary,
                full_result,
                cost_usd,
                tool_call_count,
            } => self.handle_background_agent_completed(
                task_id,
                success,
                result_summary,
                full_result,
                cost_usd,
                tool_call_count,
            ),
            AppEvent::BackgroundAgentProgress {
                task_id,
                tool_name,
                tool_count,
            } => self.handle_background_agent_progress(task_id, tool_name, tool_count),
            AppEvent::BackgroundAgentActivity { task_id, line } => {
                self.handle_background_agent_activity(task_id, line)
            }
            AppEvent::BackgroundAgentKilled { task_id } => {
                self.handle_background_agent_killed(task_id)
            }
            AppEvent::SetBackgroundAgentToken {
                task_id,
                query,
                session_id,
                interrupt_token,
            } => {
                self.handle_set_background_agent_token(task_id, query, session_id, interrupt_token)
            }

            // Team events
            AppEvent::TeamCreated {
                team_id,
                leader_name,
                member_names,
            } => self.handle_team_created(team_id, leader_name, member_names),
            AppEvent::TeamMessageSent {
                from,
                to,
                content_preview,
            } => self.handle_team_message(from, to, content_preview),
            AppEvent::TeamDeleted { team_id } => self.handle_team_deleted(team_id),

            // Undo/Redo/Share events
            AppEvent::SnapshotTaken { hash } => self.handle_snapshot_taken(hash),
            AppEvent::UndoResult { success, message } => self.handle_undo_result(success, message),
            AppEvent::RedoResult { success, message } => self.handle_redo_result(success, message),
            AppEvent::ShareResult { path } => self.handle_share_result(path),
            AppEvent::FileChanged { paths } => self.handle_file_changed(paths),
            AppEvent::SessionTitleUpdated(title) => self.handle_session_title_updated(title),

            AppEvent::CostUpdate(cost) => {
                self.state.session_cost = cost;
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
