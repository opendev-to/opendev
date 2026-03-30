//! UI rendering: main layout composition, selection highlighting, and core render orchestration.
//!
//! Popup panels and modal dialogs are in [`super::render_popups`].

use ratatui::layout;

use crate::widgets::{
    ConversationWidget, InputWidget, StatusBarWidget, ToastWidget, TodoPanelWidget,
    WelcomePanelWidget,
};

use super::App;
use super::OperationMode;

impl App {
    fn spinner_wrapped_line_count(&self, content_width: u16) -> usize {
        let widget = ConversationWidget::new(&self.state.messages, self.state.scroll_offset)
            .working_dir(&self.state.working_dir)
            .path_shortener(&self.state.path_shortener)
            .active_tools(&self.state.active_tools)
            .active_subagents(&self.state.active_subagents)
            .task_progress(self.state.task_progress.as_ref())
            .spinner_char(self.state.spinner.current())
            .compaction_active(self.state.compaction_active)
            .backgrounding_pending(self.state.backgrounding_pending)
            .thinking_verb(
                self.state.spinner.current_verb(),
                self.state.spinner.verb_fade_intensity(),
            );
        widget
            .build_spinner_lines()
            .iter()
            .map(|line| {
                let w = line.width();
                if w == 0 || content_width == 0 {
                    1
                } else {
                    w.div_ceil(content_width as usize)
                }
            })
            .sum()
    }

    pub(super) fn render(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        // Layout: conversation (flexible) | todo panel (if active) | input | status bar
        let has_todos = !self.state.todo_items.is_empty();
        let todo_height = crate::widgets::todo_panel_height(
            self.state.todo_items.len(),
            self.state.todo_expanded,
        );

        let chunks = layout::Layout::default()
            .direction(layout::Direction::Vertical)
            .constraints(
                [
                    layout::Constraint::Min(5),              // conversation
                    layout::Constraint::Length(todo_height), // todo panel
                    layout::Constraint::Length({
                        let input_lines = self.state.input_buffer.matches('\n').count() + 1;
                        (input_lines as u16 + 1).min(8) // +1 for separator, cap at 8
                    }), // input
                    layout::Constraint::Length(2),           // status bar
                ]
                .as_ref(),
            )
            .split(area);

        // Conversation
        let mode_str = match self.state.mode {
            OperationMode::Normal => "NORMAL",
            OperationMode::Plan => "PLAN",
        };

        // Show animated welcome panel when no messages (or during fade-out)
        if self.state.messages.is_empty() && !self.state.welcome_panel.fade_complete {
            let wp = WelcomePanelWidget::new(&self.state.welcome_panel)
                .version(&self.state.version)
                .mode(mode_str);
            frame.render_widget(wp, chunks[0]);
        } else {
            let mut conversation =
                ConversationWidget::new(&self.state.messages, self.state.scroll_offset)
                    .version(&self.state.version)
                    .working_dir(&self.state.working_dir)
                    .path_shortener(&self.state.path_shortener)
                    .mode(mode_str)
                    .active_tools(&self.state.active_tools)
                    .active_subagents(&self.state.active_subagents)
                    .task_progress(self.state.task_progress.as_ref())
                    .spinner_char(self.state.spinner.current())
                    .compaction_active(self.state.compaction_active)
                    .backgrounding_pending(self.state.backgrounding_pending)
                    .thinking_verb(
                        self.state.spinner.current_verb(),
                        self.state.spinner.verb_fade_intensity(),
                    );
            if !self.state.cached_lines.is_empty() {
                conversation = conversation.cached_lines(&self.state.cached_lines);
            }
            frame.render_widget(conversation, chunks[0]);

            // Selection highlight: post-render buffer pass to swap fg/bg on selected cells
            if let Some(range) = self.state.selection.range {
                let conv_area = chunks[0];
                let reserved = 1;
                let content_height = conv_area.height.saturating_sub(reserved);
                let content_width = conv_area.width.saturating_sub(1);
                let sel_content_area = ratatui::layout::Rect {
                    x: conv_area.x,
                    y: conv_area.y,
                    width: content_width,
                    height: content_height,
                };

                let spinner_lines = self.spinner_wrapped_line_count(content_width);
                let sel_total_lines: usize = self
                    .state
                    .cached_lines
                    .iter()
                    .map(|line| {
                        let w = line.width();
                        if w == 0 || content_width == 0 {
                            1
                        } else {
                            w.div_ceil(content_width as usize)
                        }
                    })
                    .sum::<usize>()
                    + spinner_lines
                    + usize::from(spinner_lines > 0 && !self.state.cached_lines.is_empty());
                let viewport_height = content_height as usize;
                let max_scroll = sel_total_lines.saturating_sub(viewport_height);
                let clamped = (self.state.scroll_offset as usize).min(max_scroll);
                let sel_actual_scroll = max_scroll.saturating_sub(clamped);

                Self::render_selection_highlight(
                    frame,
                    sel_content_area,
                    sel_actual_scroll,
                    &range,
                );
            }
        }

        // Todo panel (only if plan has todos)
        if has_todos {
            let mut todo_widget = TodoPanelWidget::new(&self.state.todo_items)
                .with_expanded(self.state.todo_expanded)
                .with_spinner_tick(self.state.todo_spinner_tick);
            if let Some(ref name) = self.state.plan_name {
                todo_widget = todo_widget.with_plan_name(name);
            }
            frame.render_widget(todo_widget, chunks[1]);
        }

        // Input
        let activity_tag = self.activity_tag();
        let user_msg_count = self
            .state
            .pending_queue
            .iter()
            .filter(|item| matches!(item, super::PendingItem::UserMessage(_)))
            .count();
        let bg_result_count = self.state.pending_queue.len() - user_msg_count;
        let input = InputWidget::new(
            &self.state.input_buffer,
            self.state.input_cursor,
            mode_str,
            user_msg_count,
            bg_result_count,
            activity_tag,
        );
        frame.render_widget(input, chunks[2]);

        // Autocomplete popup (rendered over conversation area)
        if self.state.autocomplete.is_visible() {
            self.render_autocomplete(frame, chunks[2]);
        }

        // Plan approval panel (rendered over input area when active)
        if self.plan_approval_controller.active() {
            self.render_plan_approval(frame, chunks[2]);
        }

        // Ask-user panel (rendered over input area when active)
        if self.ask_user_controller.active() {
            self.render_ask_user(frame, chunks[2]);
        }

        // Tool approval panel (rendered over input area when active)
        if self.approval_controller.active() {
            self.render_approval(frame, chunks[2]);
        }

        // Model picker panel (rendered over input area when active)
        if let Some(ref picker) = self.model_picker_controller
            && picker.active()
        {
            self.render_model_picker(frame, chunks[2]);
        }

        // Status bar
        let status = StatusBarWidget::new(
            &self.state.model,
            &self.state.working_dir,
            self.state.git_branch.as_deref(),
            self.state.tokens_used,
            self.state.tokens_limit,
            self.state.mode,
        )
        .autonomy(self.state.autonomy)
        .reasoning_level(self.state.reasoning_level)
        .context_usage_pct(self.state.context_usage_pct)
        .session_cost(self.state.session_cost)
        .session_id(self.state.session_id.as_deref())
        .mcp_status(self.state.mcp_status, self.state.mcp_has_errors)
        .background_tasks(self.state.background_task_count)
        .file_changes(self.state.file_changes)
        .spinner_char(if self.state.background_task_count > 0 {
            Some(self.state.spinner.current())
        } else {
            None
        })
        .last_completion(
            self.state
                .last_task_completion
                .as_ref()
                .map(|(id, _)| format!("[{id}] completed")),
        );
        frame.render_widget(status, chunks[3]);

        // Toast notifications (top-right corner)
        if !self.state.toasts.is_empty() {
            let toast_widget = ToastWidget::new(&self.state.toasts);
            frame.render_widget(toast_widget, area);
        }

        // Leader key indicator in status bar area
        if self.state.leader_pending {
            use ratatui::style::{Modifier, Style};
            use ratatui::text::{Line, Span};
            use ratatui::widgets::Paragraph;

            let indicator = Paragraph::new(Line::from(Span::styled(
                " C-x ",
                Style::default()
                    .fg(Self::PANEL_CYAN)
                    .add_modifier(Modifier::BOLD),
            )));
            let indicator_area = layout::Rect {
                x: area.width.saturating_sub(8),
                y: area.height.saturating_sub(1),
                width: 6,
                height: 1,
            };
            frame.render_widget(indicator, indicator_area);
        }

        // Debug panel overlay (Ctrl+D)
        if self.state.debug_panel_open {
            self.render_debug_panel(frame, area);
        }

        // Task watcher overlay (Alt+B / Ctrl+P) — centered popup ~85%×80%
        if self.state.task_watcher_open {
            // Compute bg_agent_manager task IDs that are "covered" by backgrounded subagents —
            // these parent tasks are redundant since each subagent already has its own panel.
            let covered_bg_task_ids: std::collections::HashSet<String> = self
                .state
                .active_subagents
                .iter()
                .filter(|s| s.backgrounded)
                .filter_map(|s| self.state.bg_subagent_map.get(&s.subagent_id))
                .cloned()
                .collect();
            let watcher = crate::widgets::background_tasks::TaskWatcherPanel::new(
                &self.state.active_subagents,
                &self.state.bg_agent_manager,
                &covered_bg_task_ids,
                self.state.spinner.tick_count() as usize,
                &self.state.path_shortener,
            )
            .focus(self.state.task_watcher_focus)
            .cell_scrolls(&self.state.task_watcher_cell_scrolls)
            .page(self.state.task_watcher_page);
            let w = ((area.width as f32 * 0.85) as u16).clamp(40, area.width);
            let h = ((area.height as f32 * 0.80) as u16).clamp(12, area.height);
            let x = area.x + (area.width.saturating_sub(w)) / 2;
            let y = area.y + (area.height.saturating_sub(h)) / 2;
            let panel_area = layout::Rect::new(x, y, w, h);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            frame.render_widget(watcher, panel_area);
        }
    }

    /// Compute the high-level activity tag for the input separator.
    /// Priority: active plan name > session title > nothing.
    fn activity_tag(&self) -> Option<&str> {
        if self.state.agent_active
            && let Some(ref name) = self.state.plan_name
        {
            return Some(name);
        }
        self.state.session_title.as_deref()
    }

    /// Update selection geometry from current layout state.
    /// Called after render so mouse position mapping uses fresh data.
    pub(super) fn update_selection_geometry(&mut self) {
        // Recompute the conversation content area dimensions
        let todo_height = crate::widgets::todo_panel_height(
            self.state.todo_items.len(),
            self.state.todo_expanded,
        );
        let input_lines = self.state.input_buffer.matches('\n').count() + 1;
        let input_height = (input_lines as u16 + 1).min(8);

        let total_height = self.state.terminal_height;
        let conv_height = total_height
            .saturating_sub(todo_height)
            .saturating_sub(input_height)
            .saturating_sub(2) // status bar
            .max(5);

        let reserved = 1;
        let content_height = conv_height.saturating_sub(reserved);
        let content_width = self.state.terminal_width.saturating_sub(1);

        let content_area = ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: content_width,
            height: content_height,
        };

        let spinner_lines = self.spinner_wrapped_line_count(content_width);
        let total_lines: usize = self
            .state
            .cached_lines
            .iter()
            .map(|line| {
                let w = line.width();
                if w == 0 || content_width == 0 {
                    1
                } else {
                    w.div_ceil(content_width as usize)
                }
            })
            .sum::<usize>()
            + spinner_lines
            + usize::from(spinner_lines > 0 && !self.state.cached_lines.is_empty());
        let viewport_height = content_height as usize;
        let max_scroll = total_lines.saturating_sub(viewport_height);
        let clamped = (self.state.scroll_offset as usize).min(max_scroll);
        let actual_scroll = max_scroll.saturating_sub(clamped);

        self.state.selection.conversation_area = content_area;
        self.state.selection.actual_scroll = actual_scroll;
        self.state.selection.total_content_lines = total_lines;
    }

    /// Render selection highlight by swapping fg/bg on selected buffer cells.
    fn render_selection_highlight(
        frame: &mut ratatui::Frame,
        content_area: ratatui::layout::Rect,
        actual_scroll: usize,
        range: &crate::selection::SelectionRange,
    ) {
        let buf = frame.buffer_mut();
        let (start, end) = range.ordered();

        for screen_row in 0..content_area.height {
            let line_idx = actual_scroll + screen_row as usize;
            if line_idx < start.line_index || line_idx > end.line_index {
                continue;
            }

            let col_start = if line_idx == start.line_index {
                start.char_offset as u16
            } else {
                0
            };
            let col_end = if line_idx == end.line_index {
                end.char_offset as u16
            } else {
                content_area.width
            };

            let y = content_area.y + screen_row;
            for col in col_start..col_end.min(content_area.width) {
                let x = content_area.x + col;
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(x, y)) {
                    // Swap foreground and background for highlight
                    let fg = cell.fg;
                    let bg = cell.bg;
                    cell.set_fg(if bg == ratatui::style::Color::Reset {
                        ratatui::style::Color::Black
                    } else {
                        bg
                    });
                    cell.set_bg(if fg == ratatui::style::Color::Reset {
                        ratatui::style::Color::White
                    } else {
                        fg
                    });
                }
            }
        }
    }

    /// True cyan color for popup panel borders and accents.
    pub(super) const PANEL_CYAN: ratatui::style::Color = ratatui::style::Color::Rgb(0, 255, 255);
}
