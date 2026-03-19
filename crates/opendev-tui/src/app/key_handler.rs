//! Keyboard input handling: key dispatch, modal delegation, scroll, and navigation.

use crossterm::event::{KeyCode, KeyModifiers};

use crate::event::AppEvent;
use crate::widgets::TaskWatcherFocus;

use super::{App, AutonomyLevel, OperationMode};

impl App {
    fn next_char_boundary(s: &str, pos: usize) -> usize {
        let mut idx = pos + 1;
        while idx < s.len() && !s.is_char_boundary(idx) {
            idx += 1;
        }
        idx.min(s.len())
    }

    /// Return the byte offset of the previous char boundary before `pos` in `s`,
    /// or 0 if already at the start.
    fn prev_char_boundary(s: &str, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        let mut idx = pos - 1;
        while idx > 0 && !s.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    /// Attempt to background the running agent. Returns true if initiated.
    fn try_background_agent(&mut self) -> bool {
        if !self.state.agent_active || self.state.backgrounding_pending {
            return false;
        }
        if !self.state.bg_agent_manager.can_accept() {
            self.push_system_message(format!(
                "Maximum background agents reached ({}).",
                self.state.bg_agent_manager.max_concurrent
            ));
            return false;
        }
        if let Some(ref token) = self.interrupt_token {
            token.request_background();
            self.state.backgrounding_pending = true;
            true
        } else {
            self.push_system_message(
                "Cannot background: agent token not ready yet.".to_string(),
            );
            false
        }
    }

    /// Dismiss any active modal controllers with permissive responses to unblock
    /// the react loop so it can reach the background check on the next iteration.
    fn dismiss_modals_for_background(&mut self) {
        if self.approval_controller.active() {
            let command = self.approval_controller.command().to_string();
            // Auto-approve to unblock the tool execution
            self.approval_controller.confirm();
            if let Some(tx) = self.approval_response_tx.take() {
                let _ = tx.send(opendev_runtime::ToolApprovalDecision {
                    approved: true,
                    choice: "yes".to_string(),
                    command,
                });
            }
        }
        if self.ask_user_controller.active() {
            // Send default answer to unblock
            self.ask_user_controller.cancel();
            self.ask_user_response_tx.take();
        }
        if self.plan_approval_controller.active() {
            // Auto-approve the plan to unblock
            if let Some(decision) = self.plan_approval_controller.approve() {
                self.state.mode = super::OperationMode::Normal;
                if let Some(tx) = self.plan_approval_response_tx.take() {
                    let _ = tx.send(decision);
                }
            }
        }
    }

    /// Handle a key press event.
    pub(super) fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        // Only process key-press and repeat events (Kitty protocol also sends Release)
        if !matches!(
            key.kind,
            crossterm::event::KeyEventKind::Press | crossterm::event::KeyEventKind::Repeat
        ) {
            return;
        }

        // Ctrl+B — background agent: handle before any modal can swallow it
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('b') {
            if self.try_background_agent() {
                // Dismiss any active modal with a permissive response to unblock the react loop
                self.dismiss_modals_for_background();
            }
            self.state.dirty = true;
            return;
        }

        // Delegate to model picker controller when active
        if let Some(ref mut picker) = self.model_picker_controller
            && picker.active()
        {
            match key.code {
                KeyCode::Up => picker.prev(),
                KeyCode::Down => picker.next(),
                KeyCode::Enter => {
                    if let Some(selected) = picker.select() {
                        self.state.model = selected.id.clone();
                        self.push_system_message(format!(
                            "Model switched to: {} ({})",
                            selected.name, selected.provider_display
                        ));
                        // Reset reasoning level — new model may have different support
                        self.state.reasoning_level = super::enums::ReasoningLevel::Off;
                        // Propagate to backend
                        if let Some(ref tx) = self.user_message_tx {
                            let _ = tx.send(format!("\x00__MODEL_CHANGE__{}", self.state.model));
                        }
                    }
                    self.model_picker_controller = None;
                }
                KeyCode::Esc => {
                    picker.cancel();
                    self.model_picker_controller = None;
                }
                KeyCode::Backspace => picker.search_pop(),
                KeyCode::Char(c) => picker.search_push(c),
                _ => {}
            }
            self.state.dirty = true;
            return;
        }

        // Delegate to task watcher panel when open
        if self.state.task_watcher_open {
            let task_count = self.collect_unified_tasks().len();
            match (key.modifiers, key.code) {
                (_, KeyCode::Char('q'))
                | (_, KeyCode::Esc)
                | (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                    self.state.task_watcher_open = false;
                }
                (_, KeyCode::Char('j')) | (_, KeyCode::Down) => {
                    if task_count > 0 {
                        self.state.task_watcher_selected =
                            (self.state.task_watcher_selected + 1) % task_count;
                        self.state.task_watcher_output_scroll = 0;
                    }
                }
                (_, KeyCode::Char('k')) | (_, KeyCode::Up) => {
                    if task_count > 0 {
                        self.state.task_watcher_selected = if self.state.task_watcher_selected == 0
                        {
                            task_count - 1
                        } else {
                            self.state.task_watcher_selected - 1
                        };
                        self.state.task_watcher_output_scroll = 0;
                    }
                }
                (_, KeyCode::Enter) => {
                    self.state.task_watcher_focus = match self.state.task_watcher_focus {
                        TaskWatcherFocus::List => TaskWatcherFocus::Output,
                        TaskWatcherFocus::Output => TaskWatcherFocus::List,
                    };
                }
                (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                    if self.state.task_watcher_focus == TaskWatcherFocus::Output {
                        self.state.task_watcher_output_scroll =
                            self.state.task_watcher_output_scroll.saturating_add(10);
                    }
                }
                (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                    if self.state.task_watcher_focus == TaskWatcherFocus::Output {
                        self.state.task_watcher_output_scroll =
                            self.state.task_watcher_output_scroll.saturating_sub(10);
                    }
                }
                (_, KeyCode::Char('x')) => {
                    let tasks = self.collect_unified_tasks();
                    if let Some(task) = tasks.get(self.state.task_watcher_selected)
                        && task.state == "running"
                    {
                        let task_id = task.task_id.clone();
                        match task.kind {
                            crate::widgets::UnifiedTaskKind::Agent => {
                                if self.state.bg_agent_manager.kill_task(&task_id) {
                                    let _ = self
                                        .event_tx
                                        .send(AppEvent::BackgroundAgentKilled { task_id });
                                }
                            }
                            crate::widgets::UnifiedTaskKind::Process => {
                                let _ = self.event_tx.send(AppEvent::KillTask(task_id));
                            }
                        }
                    }
                }
                (_, KeyCode::Char('d')) => {
                    let tasks = self.collect_unified_tasks();
                    if let Some(task) = tasks.get(self.state.task_watcher_selected)
                        && task.state != "running"
                    {
                        let _task_id = task.task_id.clone();
                        match task.kind {
                            crate::widgets::UnifiedTaskKind::Agent => {
                                self.state.bg_agent_manager.cleanup_old(0.0);
                            }
                            crate::widgets::UnifiedTaskKind::Process => {
                                if let Ok(mut mgr) = self.task_manager.try_lock() {
                                    mgr.remove_task(&_task_id);
                                }
                            }
                        }
                        // Adjust selection
                        let new_count = task_count.saturating_sub(1);
                        if self.state.task_watcher_selected >= new_count && new_count > 0 {
                            self.state.task_watcher_selected = new_count - 1;
                        }
                    }
                }
                _ => {}
            }
            self.state.dirty = true;
            return;
        }

        // Delegate to ask-user controller when active
        if self.ask_user_controller.active() {
            match key.code {
                KeyCode::Up => self.ask_user_controller.prev(),
                KeyCode::Down => self.ask_user_controller.next(),
                KeyCode::Enter => {
                    if let Some(_answer) = self.ask_user_controller.confirm() {
                        // confirm() already sent via the controller's internal oneshot.
                        // Clean up our stored sender (already consumed by confirm).
                        self.ask_user_response_tx.take();
                    }
                }
                KeyCode::Esc => {
                    self.ask_user_controller.cancel();
                    self.ask_user_response_tx.take();
                    let _ = self.event_tx.send(AppEvent::Interrupt);
                }
                _ => {}
            }
            self.state.dirty = true;
            return;
        }

        // Delegate to plan approval controller when active
        if self.plan_approval_controller.active() {
            match key.code {
                KeyCode::Up => self.plan_approval_controller.prev(),
                KeyCode::Down => self.plan_approval_controller.next(),
                KeyCode::Enter => {
                    if let Some(decision) = self.plan_approval_controller.confirm() {
                        // Switch mode based on decision
                        match decision.action.as_str() {
                            "approve_auto" | "approve" => {
                                self.state.mode = OperationMode::Normal;
                            }
                            _ => {} // "modify" stays in Plan mode
                        }
                        // Forward decision back to the blocking tool
                        if let Some(tx) = self.plan_approval_response_tx.take() {
                            let _ = tx.send(decision);
                        }
                    }
                }
                KeyCode::Esc => {
                    self.plan_approval_controller.cancel();
                    // cancel() internally confirms with "modify" via the controller's
                    // oneshot — but we also need to forward through our stored sender.
                    // The controller already sent via its own oneshot in cancel(),
                    // so just clean up our stored tx (it's already consumed by cancel).
                    self.plan_approval_response_tx.take();
                    let _ = self.event_tx.send(AppEvent::Interrupt);
                }
                _ => {}
            }
            self.state.dirty = true;
            return;
        }

        // Delegate to approval controller when active
        if self.approval_controller.active() {
            match key.code {
                KeyCode::Up => self.approval_controller.move_selection(-1),
                KeyCode::Down => self.approval_controller.move_selection(1),
                KeyCode::Enter => {
                    // Capture selected option before confirm() clears state
                    let idx = self.approval_controller.selected_index();
                    let option = self.approval_controller.options()[idx].clone();
                    let command = self.approval_controller.command().to_string();
                    self.approval_controller.confirm();
                    // Forward decision back to the react loop
                    if let Some(tx) = self.approval_response_tx.take() {
                        let choice = if option.choice == "2" {
                            "yes_remember".to_string()
                        } else if option.approved {
                            "yes".to_string()
                        } else {
                            "no".to_string()
                        };
                        let _ = tx.send(opendev_runtime::ToolApprovalDecision {
                            approved: option.approved,
                            choice,
                            command,
                        });
                    }
                }
                KeyCode::Esc => {
                    let command = self.approval_controller.command().to_string();
                    self.approval_controller.cancel();
                    // Send denial back to the react loop
                    if let Some(tx) = self.approval_response_tx.take() {
                        let _ = tx.send(opendev_runtime::ToolApprovalDecision {
                            approved: false,
                            choice: "no".to_string(),
                            command,
                        });
                    }
                    let _ = self.event_tx.send(AppEvent::Interrupt);
                }
                _ => {}
            }
            self.state.dirty = true;
            return;
        }

        // Leader key dispatch (Ctrl+X was pressed, waiting for second key)
        if self.state.leader_pending {
            self.state.leader_pending = false;
            self.state.leader_timestamp = None;
            match key.code {
                KeyCode::Char('u') => {
                    // Undo
                    self.execute_slash_command("/undo");
                }
                KeyCode::Char('r') => {
                    // Redo
                    self.execute_slash_command("/redo");
                }
                KeyCode::Char('s') => {
                    // Share
                    self.execute_slash_command("/share");
                }
                KeyCode::Char('m') => {
                    // Models
                    self.execute_slash_command("/models");
                }
                KeyCode::Char('p') => {
                    // Sessions
                    self.execute_slash_command("/sessions");
                }
                KeyCode::Char('d') => {
                    // Debug panel
                    self.state.debug_panel_open = !self.state.debug_panel_open;
                }
                KeyCode::Esc => {
                    // Cancel leader
                }
                _ => {
                    use crate::widgets::toast::{Toast, ToastLevel};
                    self.state.toasts.push(Toast::new(
                        format!("C-x {:?} — unknown", key.code),
                        ToastLevel::Warning,
                    ));
                }
            }
            self.state.dirty = true;
            return;
        }

        // Debug panel key handler
        if self.state.debug_panel_open {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.state.debug_panel_open = false;
                }
                _ => {}
            }
            self.state.dirty = true;
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                return;
            }
        }

        match (key.modifiers, key.code) {
            // Ctrl+C — quit or clear input
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                if self.state.input_buffer.is_empty() && !self.state.agent_active {
                    self.state.running = false;
                } else if self.state.agent_active {
                    // Interrupt agent
                    let _ = self.event_tx.send(AppEvent::Interrupt);
                } else {
                    self.state.input_buffer.clear();
                    self.state.input_cursor = 0;
                }
            }
            // Escape — dismiss autocomplete or interrupt agent
            (_, KeyCode::Esc) => {
                if self.state.autocomplete.is_visible() {
                    self.state.autocomplete.dismiss();
                } else {
                    let _ = self.event_tx.send(AppEvent::Interrupt);
                }
            }
            // Shift+Enter — insert newline in input buffer
            // iTerm2 (and many terminals) map Shift+Enter to Ctrl+J (ASCII LF).
            // Alt+Enter sends Enter with ALT modifier. Both insert a newline.
            (KeyModifiers::CONTROL, KeyCode::Char('j')) => {
                if !self.state.agent_active {
                    self.state
                        .input_buffer
                        .insert(self.state.input_cursor, '\n');
                    self.state.input_cursor += '\n'.len_utf8();
                }
            }
            (m, KeyCode::Enter)
                if m.contains(KeyModifiers::SHIFT) || m.contains(KeyModifiers::ALT) =>
            {
                if !self.state.agent_active {
                    self.state
                        .input_buffer
                        .insert(self.state.input_cursor, '\n');
                    self.state.input_cursor += '\n'.len_utf8();
                }
            }
            // Enter — accept autocomplete, submit message, or execute slash command
            (_, KeyCode::Enter) => {
                if self.state.autocomplete.is_visible() {
                    // If the input is already a known slash command, dismiss autocomplete
                    // and submit it directly — don't let autocomplete replace it.
                    let is_exact_slash = self.state.input_buffer.starts_with('/')
                        && !self.state.input_buffer[1..].contains(' ')
                        && crate::controllers::is_command(&self.state.input_buffer[1..]);

                    if is_exact_slash {
                        self.state.autocomplete.dismiss();
                        // Fall through to submit below
                    } else {
                        // Accept autocomplete selection
                        if let Some((insert_text, delete_count)) = self.state.autocomplete.accept()
                        {
                            let start = self.state.input_cursor.saturating_sub(delete_count);
                            self.state
                                .input_buffer
                                .drain(start..self.state.input_cursor);
                            self.state.input_cursor = start;
                            self.state
                                .input_buffer
                                .insert_str(self.state.input_cursor, &insert_text);
                            self.state.input_cursor += insert_text.len();
                            // Add trailing space
                            self.state.input_buffer.insert(self.state.input_cursor, ' ');
                            self.state.input_cursor += 1;
                        }
                    }
                }
                if !self.state.autocomplete.is_visible() && !self.state.input_buffer.is_empty() {
                    let msg = self.state.input_buffer.clone();
                    self.state.input_buffer.clear();
                    self.state.input_cursor = 0;
                    self.state.autocomplete.dismiss();
                    self.state.command_history.record(&msg);

                    if self.state.agent_active {
                        // Queue silently — message will display when consumed
                        self.state.pending_messages.push(msg);
                        self.state.dirty = true;
                    } else {
                        // Start fading the welcome panel on first user message
                        if !self.state.welcome_panel.fade_complete
                            && !self.state.welcome_panel.is_fading
                        {
                            self.state.welcome_panel.start_fade();
                        }

                        if msg.starts_with('/') {
                            self.execute_slash_command(&msg);
                        } else {
                            self.message_controller
                                .handle_user_submit(&mut self.state, &msg);
                            self.state.message_generation += 1;
                            let _ = self.event_tx.send(AppEvent::UserSubmit(msg));
                        }
                    }
                }
            }
            // Backspace
            (_, KeyCode::Backspace) => {
                if self.state.input_cursor > 0 {
                    self.state.input_cursor =
                        Self::prev_char_boundary(&self.state.input_buffer, self.state.input_cursor);
                    self.state.input_buffer.remove(self.state.input_cursor);
                    self.update_autocomplete();
                }
            }
            // Delete
            (_, KeyCode::Delete) => {
                if self.state.input_cursor < self.state.input_buffer.len() {
                    self.state.input_buffer.remove(self.state.input_cursor);
                    self.update_autocomplete();
                }
            }
            // Left arrow
            (_, KeyCode::Left) => {
                if self.state.input_cursor > 0 {
                    self.state.input_cursor =
                        Self::prev_char_boundary(&self.state.input_buffer, self.state.input_cursor);
                }
            }
            // Right arrow
            (_, KeyCode::Right) => {
                if self.state.input_cursor < self.state.input_buffer.len() {
                    self.state.input_cursor =
                        Self::next_char_boundary(&self.state.input_buffer, self.state.input_cursor);
                }
            }
            // Home
            (_, KeyCode::Home) => {
                self.state.input_cursor = 0;
            }
            // End
            (_, KeyCode::End) => {
                self.state.input_cursor = self.state.input_buffer.len();
            }
            // Page Up — scroll conversation (with acceleration)
            (_, KeyCode::PageUp) => {
                let base = self.accelerated_scroll(true);
                // PageUp uses 3x the accelerated scroll amount
                let amount = base.saturating_mul(3);
                self.state.scroll_offset = self.state.scroll_offset.saturating_add(amount);
                self.state.user_scrolled = true;
            }
            // Page Down — scroll conversation (with acceleration)
            (_, KeyCode::PageDown) => {
                if self.state.scroll_offset > 0 {
                    let base = self.accelerated_scroll(false);
                    let amount = base.saturating_mul(3);
                    self.state.scroll_offset = self.state.scroll_offset.saturating_sub(amount);
                } else {
                    self.state.user_scrolled = false;
                }
            }
            // Shift+Tab — cycle mode and set pending plan flag
            (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                self.state.mode = match self.state.mode {
                    OperationMode::Normal => {
                        self.state.pending_plan_request = true;
                        OperationMode::Plan
                    }
                    OperationMode::Plan => {
                        self.state.pending_plan_request = false;
                        OperationMode::Normal
                    }
                };
            }
            // Ctrl+Shift+A — cycle autonomy level
            // Kitty keyboard protocol reports base key (lowercase 'a') with SHIFT modifier;
            // legacy terminals report uppercase 'A'. Handle both.
            (m, KeyCode::Char('A' | 'a'))
                if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) =>
            {
                self.state.autonomy = match self.state.autonomy {
                    AutonomyLevel::Manual => AutonomyLevel::SemiAuto,
                    AutonomyLevel::SemiAuto => AutonomyLevel::Auto,
                    AutonomyLevel::Auto => AutonomyLevel::Manual,
                };
            }
            // Ctrl+Shift+T — cycle reasoning effort level
            (m, KeyCode::Char('T' | 't'))
                if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) =>
            {
                self.state.reasoning_level = self.state.reasoning_level.next();
                // Propagate to runtime via sentinel message
                if let Some(ref tx) = self.user_message_tx {
                    let effort = self.state.reasoning_level.to_config_string();
                    let sentinel = match effort {
                        Some(e) => format!("\x00__REASONING_EFFORT__{e}"),
                        None => "\x00__REASONING_EFFORT__none".to_string(),
                    };
                    let _ = tx.send(sentinel);
                }
            }
            // Tab — accept autocomplete suggestion, or toggle mode when input is empty
            (_, KeyCode::Tab) => {
                if let Some((insert_text, delete_count)) = self.state.autocomplete.accept() {
                    let start = self.state.input_cursor.saturating_sub(delete_count);
                    self.state
                        .input_buffer
                        .drain(start..self.state.input_cursor);
                    self.state.input_cursor = start;
                    self.state
                        .input_buffer
                        .insert_str(self.state.input_cursor, &insert_text);
                    self.state.input_cursor += insert_text.len();
                    // Add trailing space
                    self.state.input_buffer.insert(self.state.input_cursor, ' ');
                    self.state.input_cursor += 1;
                } else if self.state.input_buffer.is_empty() {
                    // Toggle mode like Shift+Tab when input is empty
                    self.state.mode = match self.state.mode {
                        OperationMode::Normal => {
                            self.state.pending_plan_request = true;
                            OperationMode::Plan
                        }
                        OperationMode::Plan => {
                            self.state.pending_plan_request = false;
                            OperationMode::Normal
                        }
                    };
                }
            }
            // Up/Down arrow — autocomplete > command history > scroll
            (_, KeyCode::Up) => {
                if self.state.autocomplete.is_visible() {
                    self.state.autocomplete.select_prev();
                } else if !self.state.input_buffer.contains('\n') && !self.state.agent_active {
                    // Single-line input: navigate command history
                    if let Some(text) = self
                        .state
                        .command_history
                        .navigate_up(&self.state.input_buffer)
                    {
                        self.state.input_buffer = text.to_string();
                        self.state.input_cursor = self.state.input_buffer.len();
                    }
                } else {
                    let amount = self.accelerated_scroll(true);
                    self.state.scroll_offset = self.state.scroll_offset.saturating_add(amount);
                    self.state.user_scrolled = true;
                }
            }
            (_, KeyCode::Down) => {
                if self.state.autocomplete.is_visible() {
                    self.state.autocomplete.select_next();
                } else if self.state.command_history.is_navigating() {
                    // Navigate command history down
                    if let Some(text) = self.state.command_history.navigate_down() {
                        self.state.input_buffer = text.to_string();
                        self.state.input_cursor = self.state.input_buffer.len();
                    }
                } else if self.state.scroll_offset > 0 {
                    let amount = self.accelerated_scroll(false);
                    self.state.scroll_offset = self.state.scroll_offset.saturating_sub(amount);
                } else {
                    self.state.user_scrolled = false;
                }
            }
            // Ctrl+O — toggle collapsed state on the most recent collapsible tool result
            (KeyModifiers::CONTROL, KeyCode::Char('o')) => {
                use crate::widgets::conversation::is_diff_tool;
                // Priority 1: Most recent collapsible tool result (excluding edit/write)
                let mut toggled = false;
                for msg in self.state.messages.iter_mut().rev() {
                    if let Some(ref mut tc) = msg.tool_call
                        && !tc.result_lines.is_empty()
                        && !is_diff_tool(&tc.name)
                    {
                        tc.collapsed = !tc.collapsed;
                        self.state.message_generation += 1;
                        toggled = true;
                        break;
                    }
                }
                let _ = toggled; // suppress unused warning
            }
            // Ctrl+T — toggle todo panel expanded/collapsed
            (KeyModifiers::CONTROL, KeyCode::Char('t')) => {
                if !self.state.todo_items.is_empty() {
                    self.state.todo_expanded = !self.state.todo_expanded;
                    self.state.dirty = true;
                }
            }
            // Alt+B / Ctrl+P — toggle task watcher panel
            (KeyModifiers::ALT, KeyCode::Char('b'))
            | (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                self.state.task_watcher_open = !self.state.task_watcher_open;
                if self.state.task_watcher_open {
                    self.state.task_watcher_selected = 0;
                    self.state.task_watcher_output_scroll = 0;
                    self.state.task_watcher_focus = TaskWatcherFocus::List;
                }
            }
            // Ctrl+X — leader key prefix
            (KeyModifiers::CONTROL, KeyCode::Char('x')) => {
                self.state.leader_pending = true;
                self.state.leader_timestamp = Some(std::time::Instant::now());
            }
            // Ctrl+D — toggle debug panel
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.state.debug_panel_open = !self.state.debug_panel_open;
            }
            // Ctrl+R — open session picker
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                self.execute_slash_command("/sessions");
            }
            // Ctrl+B handled at top of handle_key (before modals)
            // Regular character input
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                self.state.command_history.reset_navigation();
                self.state.input_buffer.insert(self.state.input_cursor, c);
                self.state.input_cursor += c.len_utf8();
                // Update autocomplete on input change
                self.update_autocomplete();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn test_handle_key_char_input() {
        let mut app = App::new();
        let key = crossterm::event::KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(key);
        assert_eq!(app.state.input_buffer, "a");
        assert_eq!(app.state.input_cursor, 1);
    }

    #[test]
    fn test_handle_key_backspace() {
        let mut app = App::new();
        app.state.input_buffer = "abc".into();
        app.state.input_cursor = 3;
        let key = crossterm::event::KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        app.handle_key(key);
        assert_eq!(app.state.input_buffer, "ab");
        assert_eq!(app.state.input_cursor, 2);
    }

    #[test]
    fn test_handle_key_enter_submits() {
        let mut app = App::new();
        app.state.input_buffer = "hello".into();
        app.state.input_cursor = 5;
        let key = crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(key);
        assert!(app.state.input_buffer.is_empty());
        assert_eq!(app.state.input_cursor, 0);
        // Should have added a user message
        assert_eq!(app.state.messages.len(), 1);
        assert_eq!(app.state.messages[0].role, DisplayRole::User);
    }

    #[test]
    fn test_mode_toggle() {
        let mut app = App::new();
        let key = crossterm::event::KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        app.handle_key(key);
        assert_eq!(app.state.mode, OperationMode::Plan);
        app.handle_key(key);
        assert_eq!(app.state.mode, OperationMode::Normal);
    }

    #[test]
    fn test_page_scroll() {
        let mut app = App::new();
        let pgup = crossterm::event::KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE);
        app.handle_key(pgup);
        // First press: base=3 (no accel), page multiplier 3x = 9
        assert_eq!(app.state.scroll_offset, 9);
        assert!(app.state.user_scrolled);

        // Page down reduces offset; direction change resets accel, so base=3, 3x=9
        let pgdn = crossterm::event::KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
        app.handle_key(pgdn);
        assert_eq!(app.state.scroll_offset, 0);
        // user_scrolled only clears when already at 0 and page down again
        assert!(app.state.user_scrolled);

        // One more page down at 0 clears user_scrolled
        app.handle_key(pgdn);
        assert!(!app.state.user_scrolled);
    }

    #[test]
    fn test_scroll_acceleration() {
        let mut app = App::new();
        // Set agent_active so Up/Down arrow scrolls (bypasses command history)
        app.state.agent_active = true;
        // First up-arrow: base amount = 3
        let up = crossterm::event::KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 3);
        assert_eq!(app.state.scroll_accel_level, 0);

        // Immediate second press (within 200ms): accelerates to 6
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 9); // 3 + 6
        assert_eq!(app.state.scroll_accel_level, 1);

        // Third press: accelerates to 12
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 21); // 9 + 12
        assert_eq!(app.state.scroll_accel_level, 2);

        // Fourth press: stays at 12 (capped)
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 33); // 21 + 12
        assert_eq!(app.state.scroll_accel_level, 2);
    }

    #[test]
    fn test_scroll_acceleration_resets_on_direction_change() {
        let mut app = App::new();
        // Set agent_active so Up/Down arrow scrolls (bypasses command history)
        app.state.agent_active = true;
        let up = crossterm::event::KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let down = crossterm::event::KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);

        // Build up acceleration
        app.handle_key(up);
        app.handle_key(up);
        assert_eq!(app.state.scroll_accel_level, 1);
        assert_eq!(app.state.scroll_offset, 9); // 3 + 6

        // Direction change resets acceleration
        app.handle_key(down);
        assert_eq!(app.state.scroll_accel_level, 0);
        assert_eq!(app.state.scroll_offset, 6); // 9 - 3
    }

    #[test]
    fn test_models_command_opens_picker_with_autocomplete() {
        let mut app = App::new();
        // Simulate typing "/models" character by character
        for c in "/models".chars() {
            let key = crossterm::event::KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            app.handle_key(key);
        }
        assert_eq!(app.state.input_buffer, "/models");

        // Autocomplete should be visible (showing /models command)
        // (It may or may not be visible depending on the completer setup in tests)

        // Press Enter — should execute /models, not accept autocomplete
        let enter = crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(enter);

        // Input should be cleared (command was submitted)
        assert!(app.state.input_buffer.is_empty());

        // Should either open picker or show "No models" message
        let has_picker = app.model_picker_controller.is_some();
        let has_no_models_msg = app
            .state
            .messages
            .iter()
            .any(|m| m.content.contains("No models"));
        assert!(
            has_picker || has_no_models_msg,
            "Expected model picker or 'No models' message, got messages: {:?}",
            app.state
                .messages
                .iter()
                .map(|m| &m.content)
                .collect::<Vec<_>>()
        );
    }
}
