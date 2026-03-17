//! Tick-based updates: animations, scroll acceleration, and autocomplete.

use std::time::{Duration, Instant};

use super::App;

impl App {
    pub(super) fn accelerated_scroll(&mut self, up: bool) -> u16 {
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
            0 => 3,
            1 => 6,
            _ => 12,
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

        // Advance spinner animation
        if self.state.agent_active || !self.state.active_tools.is_empty() {
            self.state.spinner.tick();
        }

        // Advance todo spinner (for collapsed mode)
        if !self.state.todo_items.is_empty() {
            self.state.todo_spinner_tick = self.state.todo_spinner_tick.wrapping_add(1);
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
        self.state.active_subagents.retain(|s| {
            if !s.finished {
                return true;
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
            // No matching tool — 3s grace period for display
            s.finished_at.is_some_and(|t| t.elapsed().as_secs() < 3)
        });

        // Update task progress elapsed time from wall clock
        if let Some(ref mut progress) = self.state.task_progress {
            progress.elapsed_secs = progress.started_at.elapsed().as_secs();
        }

        // Auto-scroll if user hasn't manually scrolled up
        if !self.state.user_scrolled {
            self.state.scroll_offset = 0;
        }
    }
}
