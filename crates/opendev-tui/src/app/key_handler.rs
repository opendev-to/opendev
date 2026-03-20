//! Keyboard input handling: key dispatch, modal delegation, scroll, and navigation.

use crossterm::event::{KeyCode, KeyModifiers};

use super::{App, AutonomyLevel, OperationMode};
use crate::event::AppEvent;

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
    /// Only allowed when a bash/run_command/spawn_subagent tool is actively running.
    fn try_background_agent(&mut self) -> bool {
        if !self.state.agent_active || self.state.backgrounding_pending {
            return false;
        }
        // Only allow backgrounding when a bash tool or subagent is actively running
        let has_backgroundable = self
            .state
            .active_tools
            .iter()
            .any(|t| t.name == "bash" || t.name == "run_command" || t.name == "spawn_subagent");
        if !has_backgroundable {
            use crate::widgets::toast::{Toast, ToastLevel};
            self.state.toasts.push(Toast::new(
                "No backgroundable task running".to_string(),
                ToastLevel::Warning,
            ));
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
            self.push_system_message("Cannot background: agent token not ready yet.".to_string());
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
            let fallback = self.ask_user_controller.default_value().unwrap_or_default();
            self.ask_user_controller.cancel();
            if let Some(tx) = self.ask_user_response_tx.take() {
                let _ = tx.send(fallback);
            }
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

        // [DIAG] Log all keys received while task watcher is open
        if self.state.task_watcher_open {
            tracing::warn!("[DIAG] key while watcher open: code={:?} modifiers={:?} kind={:?}", key.code, key.modifiers, key.kind);
        }

        // Ctrl+B — background agent: handle before any modal can swallow it
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('b') {
            // If task watcher is open, Ctrl+B closes it
            if self.state.task_watcher_open {
                tracing::warn!("[DIAG] Ctrl+B closing task_watcher, was={}", self.state.task_watcher_open);
                self.state.task_watcher_open = false;
                self.state.force_clear = true;
                self.state.dirty = true;
                return;
            }
            if self.try_background_agent() {
                // Dismiss any active modal with a permissive response to unblock the react loop
                self.dismiss_modals_for_background();
            }
            self.state.dirty = true;
            return;
        }

        // Ctrl+P — toggle task watcher panel: handle before any modal can swallow it
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('p') {
            if self.state.task_watcher_open {
                tracing::warn!("[DIAG] Ctrl+P closing task_watcher, was={}", self.state.task_watcher_open);
                self.state.task_watcher_open = false;
                self.state.force_clear = true;
            } else {
                let has_bg_subagents =
                    self.state.active_subagents.iter().any(|s| s.backgrounded);
                let has_bg_agents = !self.state.bg_agent_manager.all_tasks().is_empty();
                if has_bg_subagents || has_bg_agents {
                    self.state.task_watcher_open = true;
                    self.state.task_watcher_focus = 0;
                    self.state.task_watcher_cell_scrolls.clear();
                    self.state.task_watcher_page = 0;
                } else {
                    use crate::widgets::toast::{Toast, ToastLevel};
                    self.state
                        .toasts
                        .push(Toast::new("No background tasks", ToastLevel::Info));
                }
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
                        self.push_slash_echo("/models");
                        self.push_command_result(format!(
                            "Model set to {} ({})",
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

        // Delegate to task watcher overlay when open — consume all keys
        if self.state.task_watcher_open {
            // Compute covered bg task IDs (parent tasks with backgrounded subagents)
            let covered_bg_task_ids: std::collections::HashSet<&String> = self
                .state
                .active_subagents
                .iter()
                .filter(|s| s.backgrounded && !s.finished)
                .filter_map(|s| self.state.bg_subagent_map.get(&s.subagent_id))
                .collect();
            let filtered_bg_count = self
                .state
                .bg_agent_manager
                .all_tasks()
                .iter()
                .filter(|t| !covered_bg_task_ids.contains(&t.task_id))
                .count();
            let total_tasks = self.state.active_subagents.len() + filtered_bg_count;

            match (key.modifiers, key.code) {
                // Close
                (_, KeyCode::Char('q'))
                | (_, KeyCode::Esc)
                | (KeyModifiers::ALT, KeyCode::Char('b')) => {
                    tracing::warn!("[DIAG] overlay q/Esc/Alt+B closing task_watcher, modifiers={:?} code={:?}", key.modifiers, key.code);
                    self.state.task_watcher_open = false;
                    self.state.force_clear = true;
                }

                // Focus navigation: left
                (_, KeyCode::Char('h')) | (_, KeyCode::Left) => {
                    if self.state.task_watcher_focus > 0 {
                        self.state.task_watcher_focus -= 1;
                    }
                }
                // Focus navigation: right
                (_, KeyCode::Char('l')) | (_, KeyCode::Right) => {
                    if total_tasks > 0 {
                        self.state.task_watcher_focus =
                            (self.state.task_watcher_focus + 1).min(total_tasks - 1);
                    }
                }
                // Focus navigation: up (move by cols)
                (_, KeyCode::Char('k')) | (_, KeyCode::Up) => {
                    let cols = crate::widgets::background_tasks::compute_grid_cols(
                        total_tasks,
                        self.state.terminal_width,
                    );
                    if self.state.task_watcher_focus >= cols {
                        self.state.task_watcher_focus -= cols;
                    }
                }
                // Focus navigation: down (move by cols)
                (_, KeyCode::Char('j')) | (_, KeyCode::Down) => {
                    let cols = crate::widgets::background_tasks::compute_grid_cols(
                        total_tasks,
                        self.state.terminal_width,
                    );
                    let new_focus = self.state.task_watcher_focus + cols;
                    if total_tasks > 0 {
                        self.state.task_watcher_focus = new_focus.min(total_tasks - 1);
                    }
                }

                // Scroll within focused cell: up
                (KeyModifiers::SHIFT, KeyCode::Char('K')) => {
                    let idx = self.state.task_watcher_focus;
                    while self.state.task_watcher_cell_scrolls.len() <= idx {
                        self.state.task_watcher_cell_scrolls.push(0);
                    }
                    self.state.task_watcher_cell_scrolls[idx] += 1;
                }
                // Scroll within focused cell: down
                (KeyModifiers::SHIFT, KeyCode::Char('J')) => {
                    let idx = self.state.task_watcher_focus;
                    if let Some(scroll) = self.state.task_watcher_cell_scrolls.get_mut(idx) {
                        *scroll = scroll.saturating_sub(1);
                    }
                }

                // Kill focused background task
                (_, KeyCode::Char('x')) => {
                    let sa_count = self.state.active_subagents.len();
                    let focus = self.state.task_watcher_focus;
                    if focus >= sa_count {
                        let bg_idx = focus - sa_count;
                        let bg_tasks = self.state.bg_agent_manager.all_tasks();
                        if bg_idx < bg_tasks.len() {
                            let task_id = bg_tasks[bg_idx].task_id.clone();
                            self.state.bg_agent_manager.kill_task(&task_id);
                        }
                    }
                }

                // Page navigation: left
                (KeyModifiers::SHIFT, KeyCode::Char('H')) => {
                    self.state.task_watcher_page = self.state.task_watcher_page.saturating_sub(1);
                }
                // Page navigation: right
                (KeyModifiers::SHIFT, KeyCode::Char('L')) => {
                    self.state.task_watcher_page += 1; // clamped in render
                }

                _ => {}
            }
            self.state.dirty = true;
            return;
        }

        // Delegate to ask-user controller when active
        if self.ask_user_controller.active() {
            match key.code {
                KeyCode::Up if self.ask_user_controller.has_options() => {
                    self.ask_user_controller.prev();
                }
                KeyCode::Down if self.ask_user_controller.has_options() => {
                    self.ask_user_controller.next();
                }
                KeyCode::Char(c) if !self.ask_user_controller.has_options() => {
                    self.ask_user_controller.push_char(c);
                }
                KeyCode::Backspace if !self.ask_user_controller.has_options() => {
                    self.ask_user_controller.pop_char();
                }
                KeyCode::Enter => {
                    if let Some(answer) = self.ask_user_controller.confirm()
                        && let Some(tx) = self.ask_user_response_tx.take()
                    {
                        let _ = tx.send(answer);
                    }
                }
                KeyCode::Esc => {
                    let fallback = self.ask_user_controller.default_value().unwrap_or_default();
                    self.ask_user_controller.cancel();
                    if let Some(tx) = self.ask_user_response_tx.take() {
                        let _ = tx.send(fallback);
                    }
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
            // Alt+B — toggle task watcher subpanel
            (KeyModifiers::ALT, KeyCode::Char('b')) => {
                if self.state.task_watcher_open {
                    self.state.task_watcher_open = false;
                    self.state.force_clear = true;
                } else {
                    let has_bg_subagents =
                        self.state.active_subagents.iter().any(|s| s.backgrounded);
                    let has_bg_agents = !self.state.bg_agent_manager.all_tasks().is_empty();
                    if has_bg_subagents || has_bg_agents {
                        self.state.task_watcher_open = true;
                        self.state.task_watcher_focus = 0;
                        self.state.task_watcher_cell_scrolls.clear();
                        self.state.task_watcher_page = 0;
                    } else {
                        use crate::widgets::toast::{Toast, ToastLevel};
                        self.state
                            .toasts
                            .push(Toast::new("No background tasks", ToastLevel::Info));
                    }
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
            // Ctrl+Shift+R — force full screen redraw
            // (Ctrl+L is intercepted by macOS Terminal.app as "Clear to Previous Mark")
            (m, KeyCode::Char('R' | 'r'))
                if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) =>
            {
                self.state.force_clear = true;
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
    fn test_task_watcher_close_q() {
        let mut app = App::new();
        app.state.task_watcher_open = true;
        let key = crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        app.handle_key(key);
        assert!(!app.state.task_watcher_open, "q should close task watcher");
        assert!(app.state.force_clear, "q should set force_clear");
    }

    #[test]
    fn test_task_watcher_close_esc() {
        let mut app = App::new();
        app.state.task_watcher_open = true;
        let key = crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        app.handle_key(key);
        assert!(!app.state.task_watcher_open, "Esc should close task watcher");
        assert!(app.state.force_clear, "Esc should set force_clear");
    }

    #[test]
    fn test_task_watcher_close_ctrl_b() {
        let mut app = App::new();
        app.state.task_watcher_open = true;
        let key = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
        app.handle_key(key);
        assert!(!app.state.task_watcher_open, "Ctrl+B should close task watcher");
        assert!(app.state.force_clear, "Ctrl+B should set force_clear");
    }

    #[test]
    fn test_task_watcher_close_ctrl_p() {
        let mut app = App::new();
        app.state.task_watcher_open = true;
        let key = crossterm::event::KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        app.handle_key(key);
        assert!(!app.state.task_watcher_open, "Ctrl+P should close task watcher");
        assert!(app.state.force_clear, "Ctrl+P should set force_clear");
    }

    #[test]
    fn test_task_watcher_close_alt_b() {
        let mut app = App::new();
        app.state.task_watcher_open = true;
        let key = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT);
        app.handle_key(key);
        assert!(!app.state.task_watcher_open, "Alt+B should close task watcher");
        assert!(app.state.force_clear, "Alt+B should set force_clear");
    }

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
        // First press: base=1 (no accel), page multiplier 3x = 3
        assert_eq!(app.state.scroll_offset, 3);
        assert!(app.state.user_scrolled);

        // Page down reduces offset; direction change resets accel, so base=1, 3x=3
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
        // First up-arrow: base amount = 1
        let up = crossterm::event::KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 1);
        assert_eq!(app.state.scroll_accel_level, 0);

        // Immediate second press (within 200ms): accelerates to 2
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 3); // 1 + 2
        assert_eq!(app.state.scroll_accel_level, 1);

        // Third press: accelerates to 3
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 6); // 3 + 3
        assert_eq!(app.state.scroll_accel_level, 2);

        // Fourth press: stays at 3 (capped)
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 9); // 6 + 3
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
        assert_eq!(app.state.scroll_offset, 3); // 1 + 2

        // Direction change resets acceleration
        app.handle_key(down);
        assert_eq!(app.state.scroll_accel_level, 0);
        assert_eq!(app.state.scroll_offset, 2); // 3 - 1
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
