//! Todo progress panel widget.
//!
//! Displays a compact panel showing plan execution progress with
//! a progress bar and per-item status indicators. Supports both
//! expanded (full list) and collapsed (single-line spinner) modes.
//!
//! Mirrors Python's `TaskProgressDisplay` from
//! `opendev/ui_textual/components/task_progress.py`.

use crate::formatters::style_tokens;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// Spinner frames for the active-todo display (rotating arrow cycle).
const SPINNER_FRAMES: &[char] = &['→', '↘', '↓', '↙', '←', '↖', '↑', '↗'];

/// Compute the todo panel height from item count and expanded state.
///
/// Shared helper so callers don't duplicate the formula.
/// Returns 0 when `item_count` is 0 (no panel).
pub fn todo_panel_height(item_count: usize, expanded: bool) -> u16 {
    if item_count == 0 {
        return 0;
    }
    if !expanded {
        return 3;
    }
    // 2 borders + items (capped at 12 total rows)
    (item_count as u16 + 2).min(12)
}

/// Status of a single todo item for display purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoDisplayStatus {
    Pending,
    InProgress,
    Completed,
}

/// A todo item prepared for display in the panel.
#[derive(Debug, Clone)]
pub struct TodoDisplayItem {
    pub id: usize,
    pub title: String,
    pub status: TodoDisplayStatus,
    /// Present continuous text for spinner (e.g., "Running tests").
    pub active_form: Option<String>,
}

/// Widget that renders a todo progress panel.
///
/// Shows:
/// - Expanded: A title with progress count, progress bar, and each todo with status
/// - Collapsed: A single line with spinner showing the active todo's `active_form`
pub struct TodoPanelWidget<'a> {
    items: &'a [TodoDisplayItem],
    plan_name: Option<&'a str>,
    expanded: bool,
    spinner_tick: usize,
}

impl<'a> TodoPanelWidget<'a> {
    /// Create a new todo panel widget (expanded by default).
    pub fn new(items: &'a [TodoDisplayItem]) -> Self {
        Self {
            items,
            plan_name: None,
            expanded: true,
            spinner_tick: 0,
        }
    }

    /// Set the plan name to display in the title.
    pub fn with_plan_name(mut self, name: &'a str) -> Self {
        self.plan_name = Some(name);
        self
    }

    /// Set expanded/collapsed state.
    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Set the spinner tick for animation.
    pub fn with_spinner_tick(mut self, tick: usize) -> Self {
        self.spinner_tick = tick;
        self
    }

    /// Get the required height for this widget.
    /// Returns 0 when all items are completed (hides the panel, matching Python behavior).
    pub fn required_height(&self) -> u16 {
        // Hide panel when all items are done (Python: todo_panel.py:89-97)
        if !self.items.is_empty()
            && self
                .items
                .iter()
                .all(|i| i.status == TodoDisplayStatus::Completed)
        {
            return 0;
        }
        if !self.expanded {
            // Collapsed: border top + 1 line + border bottom
            return 3;
        }
        // Expanded: 2 borders + items (capped at 10)
        (self.items.len() as u16 + 2).min(12)
    }

    /// Count (done, in_progress, total) in a single pass.
    fn counts(&self) -> (usize, usize, usize) {
        let mut done = 0usize;
        let mut in_progress = 0usize;
        for item in self.items {
            match item.status {
                TodoDisplayStatus::Completed => done += 1,
                TodoDisplayStatus::InProgress => in_progress += 1,
                TodoDisplayStatus::Pending => {}
            }
        }
        (done, in_progress, self.items.len())
    }

    fn build_lines(&self, _done: usize, _in_progress: usize, _total: usize) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

        // Individual items
        for item in self.items {
            let (symbol, style) = match item.status {
                TodoDisplayStatus::Completed => (
                    " \u{2714} ".to_string(), // checkmark
                    Style::default().fg(style_tokens::GOLD),
                ),
                TodoDisplayStatus::InProgress => {
                    let spinner = SPINNER_FRAMES[self.spinner_tick % SPINNER_FRAMES.len()];
                    (
                        format!(" {spinner} "),
                        Style::default()
                            .fg(style_tokens::PRIMARY)
                            .add_modifier(Modifier::BOLD),
                    )
                }
                TodoDisplayStatus::Pending => (
                    " \u{25CB} ".to_string(), // circle
                    Style::default().fg(style_tokens::GREY),
                ),
            };

            let display_title = item.title.clone();

            lines.push(Line::from(vec![
                Span::styled(symbol, style),
                Span::styled(display_title, style),
            ]));
        }

        lines
    }

    fn build_collapsed_line(&self, done: usize, total: usize) -> Line<'a> {
        // All tasks complete — show checkmark instead of spinner
        if done == total && total > 0 {
            return Line::from(vec![
                Span::styled(
                    " \u{2714} ".to_string(),
                    Style::default()
                        .fg(style_tokens::SUCCESS)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "All tasks complete".to_string(),
                    Style::default().fg(style_tokens::SUCCESS),
                ),
                Span::styled(
                    format!("  ({done}/{total})"),
                    Style::default().fg(style_tokens::GREY),
                ),
            ]);
        }

        let spinner = SPINNER_FRAMES[self.spinner_tick % SPINNER_FRAMES.len()];

        // Find the active (doing) item
        let active_text = self
            .items
            .iter()
            .find(|i| i.status == TodoDisplayStatus::InProgress)
            .and_then(|i| {
                i.active_form
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .or(Some(i.title.as_str()))
            })
            .unwrap_or("Working...");

        Line::from(vec![
            Span::styled(
                format!(" {spinner} "),
                Style::default()
                    .fg(style_tokens::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                active_text.to_string(),
                Style::default().fg(style_tokens::PRIMARY),
            ),
            Span::styled(
                format!("  ({done}/{total})"),
                Style::default().fg(style_tokens::GREY),
            ),
        ])
    }
}

impl Widget for TodoPanelWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (done, in_progress, total) = self.counts();

        let title_text = if self.expanded {
            if let Some(name) = self.plan_name {
                format!("TODOS: {name} ({done}/{total})")
            } else {
                format!("TODOS ({done}/{total})")
            }
        } else {
            format!("TODOS ({done}/{total})")
        };

        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                title_text,
                Style::default()
                    .fg(style_tokens::GREEN_LIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " (Ctrl+T to toggle) ",
                Style::default().fg(style_tokens::GREY),
            ),
        ]);

        let border_color = if done == total && total > 0 {
            style_tokens::SUCCESS
        } else {
            style_tokens::GREY
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if self.expanded {
            let lines = self.build_lines(done, in_progress, total);
            let paragraph = Paragraph::new(lines).block(block);
            paragraph.render(area, buf);
        } else {
            let line = self.build_collapsed_line(done, total);
            let paragraph = Paragraph::new(vec![line]).block(block);
            paragraph.render(area, buf);
        }
    }
}

#[cfg(test)]
#[path = "todo_panel_tests.rs"]
mod tests;
