//! Tick-based updates: animations, scroll acceleration, and autocomplete.

use std::time::{Duration, Instant};

use super::App;

impl App {
    pub(super) fn accelerated_scroll(&mut self, up: bool) -> u32 {
        let now = Instant::now();
        let same_direction = self.state.scroll_last_direction == Some(up);
        let within_window = self
            .state
            .scroll_last_time
            .is_some_and(|t| now.duration_since(t) < Duration::from_millis(200));

        if same_direction && within_window {
            self.state.scroll_accel_level = (self.state.scroll_accel_level + 1).min(2);
        } else {
            self.state.scroll_accel_level = 0;
        }

        self.state.scroll_last_direction = Some(up);
        self.state.scroll_last_time = Some(now);

        match self.state.scroll_accel_level {
            0 => 1,
            1 => 2,
            _ => 3,
        }
    }

    /// Update autocomplete suggestions based on current input.
    pub(super) fn update_autocomplete(&mut self) {
        if self.state.agent_active {
            self.state.autocomplete.dismiss();
            return;
        }
        let text_before_cursor = self.state.input_buffer[..self.state.input_cursor].to_string();
        self.state.autocomplete.update(&text_before_cursor);
    }

    pub(super) fn handle_tick(&mut self) {
        // Advance welcome panel animation
        if !self.state.welcome_panel.fade_complete {
            // Ensure rain field is initialized/resized before ticking
            let w = self.state.terminal_width;
            let h = self.state.terminal_height;
            let rain_w = ((w as f32 * 0.7) as usize).clamp(20, 90);
            let rain_h = (h.saturating_sub(11) as usize).clamp(4, 20);
            self.state.welcome_panel.ensure_rain_field(rain_w, rain_h);
            self.state.welcome_panel.tick(w, h);
        }

        // Advance spinner animation (only count unfinished subagents so
        // the spinner stops promptly instead of persisting through the
        // finished-subagent grace period)
        if self.state.agent_active
            || !self.state.active_tools.is_empty()
            || self.state.active_subagents.iter().any(|s| !s.finished)
            || self.state.background_task_count > 0
        {
            self.state.spinner.tick();
        }

        // Invalidate cache during active thinking for animated elapsed timer
        if self
            .state
            .messages
            .iter()
            .rev()
            .any(|m| m.role == super::DisplayRole::Reasoning && m.thinking_duration_secs.is_none())
        {
            self.state.message_generation += 1;
        }

        // Advance todo spinner (for collapsed mode) — stop when all complete
        if !self.state.todo_items.is_empty() {
            let all_done = self
                .state
                .todo_items
                .iter()
                .all(|i| i.status == crate::widgets::TodoDisplayStatus::Completed);
            if !all_done {
                self.state.todo_spinner_tick = self.state.todo_spinner_tick.wrapping_add(1);
            }
        }

        // Update elapsed time and tick counter on active tools
        for tool in &mut self.state.active_tools {
            if !tool.is_finished() {
                tool.elapsed_secs = tool.started_at.elapsed().as_secs();
                tool.tick_count += 1;
            }
        }

        // Animate active subagents and clean up finished ones
        for subagent in &mut self.state.active_subagents {
            if !subagent.finished {
                subagent.advance_tick();
            }
        }
        // Remove subagents that finished more than 3 seconds ago
        // Use finished_at (not started_at) so long-running subagents aren't cleaned up immediately
        // Keep finished subagents if a matching spawn_subagent tool is still active —
        // ToolResult will consume them and extract stats before cleanup
        let active_tools = &self.state.active_tools;
        let task_watcher_open = self.state.task_watcher_open;
        self.state.active_subagents.retain(|s| {
            if !s.finished {
                return true;
            }
            // Backgrounded subagents: keep with extended grace so task watcher shows them
            if s.backgrounded {
                let grace = if task_watcher_open { 60 } else { 5 };
                return s.finished_at.is_some_and(|t| t.elapsed().as_secs() < grace);
            }
            // Keep if a matching spawn_subagent tool is still active (ToolResult will clean up).
            // Match by parent_tool_id (reliable) first, fall back to task text.
            let has_active_tool = if let Some(ref ptid) = s.parent_tool_id {
                active_tools.iter().any(|t| t.id == *ptid)
            } else {
                active_tools.iter().any(|t| {
                    t.name == "spawn_subagent"
                        && t.args.get("task").and_then(|v| v.as_str()) == Some(&s.task)
                })
            };
            if has_active_tool {
                return true;
            }
            // No matching tool — extended grace period when task watcher is open
            let grace = if task_watcher_open { 60 } else { 1 };
            s.finished_at.is_some_and(|t| t.elapsed().as_secs() < grace)
        });

        // Orphan cleanup: remove unfinished subagents whose parent tool has been removed.
        // This handles the race where SubagentStarted's fallback creates a new entry between
        // ToolResult (which removed the original entry) and ToolFinished (which removes the
        // parent tool), leaving an orphan that would otherwise persist indefinitely.
        let active_tools = &self.state.active_tools;
        self.state.active_subagents.retain(|s| {
            if s.finished || s.backgrounded {
                return true; // handled by the block above
            }
            match &s.parent_tool_id {
                Some(ptid) => active_tools.iter().any(|t| t.id == *ptid),
                None => s.started_at.elapsed().as_secs() < 30, // safety timeout
            }
        });

        // Ctrl+B hint: show after 2s of foreground subagent execution
        for subagent in &mut self.state.active_subagents {
            if !subagent.finished
                && !subagent.backgrounded
                && !subagent.background_hint_shown
                && !subagent.run_in_background
                && let Some(start) = subagent.foreground_start
                && start.elapsed() > Duration::from_secs(2)
            {
                subagent.background_hint_shown = true;
                self.state.dirty = true;
            }
        }

        // Update task progress elapsed time from wall clock
        if let Some(ref mut progress) = self.state.task_progress {
            progress.elapsed_secs = progress.started_at.elapsed().as_secs();
        }

        // Update unified background task count (both managers + backgrounded subagents)
        let bg_agent_running = self.state.bg_agent_manager.running_count();
        let bg_process_running = if let Ok(mgr) = self.task_manager.try_lock() {
            mgr.running_count()
        } else {
            0
        };
        let bg_subagent_running = self
            .state
            .active_subagents
            .iter()
            .filter(|s| s.backgrounded && !s.finished)
            .count();
        // Subtract parent bg_agent_manager tasks that are "covered" by backgrounded subagents
        // to avoid double-counting (the subagents are already counted individually).
        let covered_bg_count: usize = {
            let covered_ids: std::collections::HashSet<&String> = self
                .state
                .active_subagents
                .iter()
                .filter(|s| s.backgrounded && !s.finished)
                .filter_map(|s| self.state.bg_subagent_map.get(&s.subagent_id))
                .collect();
            covered_ids
                .iter()
                .filter(|id| {
                    self.state
                        .bg_agent_manager
                        .get_task(id)
                        .is_some_and(|t| t.is_running())
                })
                .count()
        };
        self.state.background_task_count =
            bg_agent_running + bg_process_running + bg_subagent_running - covered_bg_count;

        // Auto-close task watcher panel when all tasks finish (3s grace)
        if self.state.task_watcher_open {
            let has_running = self.state.background_task_count > 0;
            if has_running {
                self.state.task_watcher_all_done_at = None;
            } else if self.state.task_watcher_all_done_at.is_none() {
                self.state.task_watcher_all_done_at = Some(Instant::now());
            } else if self
                .state
                .task_watcher_all_done_at
                .is_some_and(|t| t.elapsed() > Duration::from_secs(3))
            {
                self.state.task_watcher_open = false;
                self.state.task_watcher_all_done_at = None;
                self.state.force_clear = true;
            }
        }

        // Clear task completion flash after 3 seconds
        if let Some((_, when)) = &self.state.last_task_completion
            && when.elapsed() > Duration::from_secs(3)
        {
            self.state.last_task_completion = None;
        }

        // Clear backgrounded task info after 3 seconds
        if let Some((_, when)) = &self.state.backgrounded_task_info
            && when.elapsed() > Duration::from_secs(3)
        {
            self.state.backgrounded_task_info = None;
        }

        // Expire old toasts
        self.state.toasts.retain(|t| !t.is_expired());
        if !self.state.toasts.is_empty() {
            self.state.dirty = true;
        }

        // Auto-cancel leader key after 2 seconds
        if self.state.leader_pending {
            if let Some(ts) = self.state.leader_timestamp
                && ts.elapsed() > Duration::from_secs(2)
            {
                self.state.leader_pending = false;
                self.state.leader_timestamp = None;
            }
            self.state.dirty = true;
        }

        // Auto-scroll during active selection drag near edges
        if self.state.selection.active
            && let Some(direction) = self.state.selection.auto_scroll_direction
        {
            if direction < 0 {
                // Scroll up (increase offset = show earlier content)
                self.state.scroll_offset = self.state.scroll_offset.saturating_add(1);
                self.state.user_scrolled = true;
            } else {
                // Scroll down (decrease offset = show later content)
                if self.state.scroll_offset > 0 {
                    self.state.scroll_offset = self.state.scroll_offset.saturating_sub(1);
                }
            }
            self.state.dirty = true;
        }

        // Expire Ctrl+C pending after 2 seconds
        if self
            .state
            .ctrl_c_pending
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
        {
            self.state.ctrl_c_pending = None;
        }

        // Auto-scroll if user hasn't manually scrolled up
        if !self.state.user_scrolled {
            self.state.scroll_offset = 0;
        }
    }
}
