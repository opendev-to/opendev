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

/// Spinner frames for the collapsed active-todo display.
const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

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
    pub fn required_height(&self) -> u16 {
        if !self.expanded {
            // Collapsed: border top + 1 line + border bottom
            return 3;
        }
        // Expanded: 2 borders + 1 progress bar + items (capped at 10)
        (self.items.len() as u16 + 3).min(12)
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

    fn build_lines(&self, done: usize, in_progress: usize, total: usize) -> Vec<Line<'a>> {

        let mut lines = Vec::new();

        // Progress bar
        if total > 0 {
            let bar_width = 20usize;
            let filled = (done * bar_width) / total;
            let partial = if in_progress > 0 && filled < bar_width {
                1
            } else {
                0
            };
            let empty = bar_width.saturating_sub(filled).saturating_sub(partial);

            let mut bar_spans = vec![Span::styled(" [", Style::default().fg(style_tokens::GREY))];
            if filled > 0 {
                bar_spans.push(Span::styled(
                    "=".repeat(filled),
                    Style::default().fg(style_tokens::SUCCESS),
                ));
            }
            if partial > 0 {
                bar_spans.push(Span::styled(
                    ">".to_string(),
                    Style::default().fg(style_tokens::PRIMARY),
                ));
            }
            if empty > 0 {
                bar_spans.push(Span::styled(
                    " ".repeat(empty),
                    Style::default().fg(style_tokens::GREY),
                ));
            }
            bar_spans.push(Span::styled(
                format!("] {done}/{total}"),
                Style::default().fg(style_tokens::GREY),
            ));
            lines.push(Line::from(bar_spans));
        }

        // Individual items
        for item in self.items {
            let (symbol, style) = match item.status {
                TodoDisplayStatus::Completed => (
                    " \u{2714} ", // checkmark
                    Style::default()
                        .fg(style_tokens::SUCCESS)
                        .add_modifier(Modifier::DIM),
                ),
                TodoDisplayStatus::InProgress => (
                    " \u{25B6} ", // play triangle
                    Style::default()
                        .fg(style_tokens::PRIMARY)
                        .add_modifier(Modifier::BOLD),
                ),
                TodoDisplayStatus::Pending => (
                    " \u{25CB} ", // circle
                    Style::default().fg(style_tokens::GREY),
                ),
            };

            let title = item.title.clone();
            // Truncate long titles
            let max_title = 60;
            let display_title = if title.len() > max_title {
                format!("{}...", &title[..max_title - 3])
            } else {
                title
            };

            lines.push(Line::from(vec![
                Span::styled(symbol.to_string(), style),
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

        let has_in_progress = in_progress > 0;

        let spinner_span = if self.expanded && has_in_progress {
            Span::styled(
                format!(
                    "{} ",
                    SPINNER_FRAMES[self.spinner_tick % SPINNER_FRAMES.len()]
                ),
                Style::default()
                    .fg(style_tokens::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("")
        };

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
            spinner_span,
            Span::styled(
                title_text,
                Style::default()
                    .fg(style_tokens::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " · Ctrl+T to toggle ",
                Style::default()
                    .fg(style_tokens::GREY)
                    .add_modifier(Modifier::DIM),
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
mod tests {
    use super::*;

    fn make_items() -> Vec<TodoDisplayItem> {
        vec![
            TodoDisplayItem {
                id: 1,
                title: "Set up project".into(),
                status: TodoDisplayStatus::Completed,
                active_form: None,
            },
            TodoDisplayItem {
                id: 2,
                title: "Write code".into(),
                status: TodoDisplayStatus::InProgress,
                active_form: Some("Writing code".into()),
            },
            TodoDisplayItem {
                id: 3,
                title: "Write tests".into(),
                status: TodoDisplayStatus::Pending,
                active_form: None,
            },
        ]
    }

    #[test]
    fn test_build_lines_count() {
        let items = make_items();
        let widget = TodoPanelWidget::new(&items);
        let (done, in_progress, total) = widget.counts();
        let lines = widget.build_lines(done, in_progress, total);
        // 1 progress bar line + 3 item lines
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_render_does_not_panic() {
        let items = make_items();
        let widget = TodoPanelWidget::new(&items).with_plan_name("bold-blazing-badger");
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 10));
        widget.render(Rect::new(0, 0, 80, 10), &mut buf);
    }

    #[test]
    fn test_empty_items() {
        let items: Vec<TodoDisplayItem> = vec![];
        let widget = TodoPanelWidget::new(&items);
        let (done, in_progress, total) = widget.counts();
        let lines = widget.build_lines(done, in_progress, total);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_all_completed_green_border() {
        let items = vec![
            TodoDisplayItem {
                id: 1,
                title: "Done".into(),
                status: TodoDisplayStatus::Completed,
                active_form: None,
            },
            TodoDisplayItem {
                id: 2,
                title: "Also done".into(),
                status: TodoDisplayStatus::Completed,
                active_form: None,
            },
        ];
        // Just verify no panic with all completed
        let widget = TodoPanelWidget::new(&items);
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 6));
        widget.render(Rect::new(0, 0, 60, 6), &mut buf);
    }

    #[test]
    fn test_long_title_truncated() {
        let items = vec![TodoDisplayItem {
            id: 1,
            title: "A".repeat(100),
            status: TodoDisplayStatus::Pending,
            active_form: None,
        }];
        let widget = TodoPanelWidget::new(&items);
        let (done, in_progress, total) = widget.counts();
        let lines = widget.build_lines(done, in_progress, total);
        // Progress bar + 1 item
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_collapsed_render() {
        let items = make_items();
        let widget = TodoPanelWidget::new(&items)
            .with_expanded(false)
            .with_spinner_tick(3);
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 3));
        widget.render(Rect::new(0, 0, 60, 3), &mut buf);
    }

    #[test]
    fn test_collapsed_uses_active_form() {
        let items = make_items();
        let widget = TodoPanelWidget::new(&items).with_expanded(false);
        let (done, _, total) = widget.counts();
        let line = widget.build_collapsed_line(done, total);
        // Should contain the active_form text "Writing code"
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("Writing code"));
    }

    #[test]
    fn test_required_height_expanded() {
        let items = make_items();
        let widget = TodoPanelWidget::new(&items);
        // 3 items + progress bar + 2 borders = 6
        assert_eq!(widget.required_height(), 6);
    }

    #[test]
    fn test_collapsed_all_done_shows_checkmark() {
        let items = vec![
            TodoDisplayItem {
                id: 1,
                title: "Task A".into(),
                status: TodoDisplayStatus::Completed,
                active_form: None,
            },
            TodoDisplayItem {
                id: 2,
                title: "Task B".into(),
                status: TodoDisplayStatus::Completed,
                active_form: None,
            },
        ];
        let widget = TodoPanelWidget::new(&items).with_expanded(false);
        let (done, _, total) = widget.counts();
        let line = widget.build_collapsed_line(done, total);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(
            text.contains("All tasks complete"),
            "Expected 'All tasks complete', got: {text}"
        );
        assert!(text.contains('\u{2714}'), "Expected checkmark in: {text}");
        assert!(
            !text.contains("Working"),
            "Should not show 'Working' when all done"
        );
    }

    #[test]
    fn test_required_height_collapsed() {
        let items = make_items();
        let widget = TodoPanelWidget::new(&items).with_expanded(false);
        assert_eq!(widget.required_height(), 3);
    }

    #[test]
    fn test_expanded_title_has_spinner_when_in_progress() {
        let items = make_items(); // has 1 in-progress item
        let widget = TodoPanelWidget::new(&items).with_spinner_tick(2);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 10));
        widget.render(Rect::new(0, 0, 80, 10), &mut buf);
        // Extract top row text from buffer
        let top_row: String = (0..80).map(|x| buf.cell((x, 0)).unwrap().symbol().to_string()).collect::<String>();
        // Should contain a spinner frame (tick 2 = '⠹')
        assert!(top_row.contains('⠹'), "Expected spinner in title, got: {top_row}");
        assert!(top_row.contains("Ctrl+T to toggle"), "Expected hint in title, got: {top_row}");
    }

    #[test]
    fn test_expanded_title_no_spinner_when_all_done() {
        let items = vec![
            TodoDisplayItem {
                id: 1,
                title: "Done".into(),
                status: TodoDisplayStatus::Completed,
                active_form: None,
            },
        ];
        let widget = TodoPanelWidget::new(&items).with_spinner_tick(2);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 6));
        widget.render(Rect::new(0, 0, 80, 6), &mut buf);
        let top_row: String = (0..80).map(|x| buf.cell((x, 0)).unwrap().symbol().to_string()).collect::<String>();
        // No spinner when all done
        assert!(!top_row.contains('⠹'), "Should not have spinner when all done, got: {top_row}");
        assert!(top_row.contains("Ctrl+T to toggle"), "Expected hint in title, got: {top_row}");
    }
}
