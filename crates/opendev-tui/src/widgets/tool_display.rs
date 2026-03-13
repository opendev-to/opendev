//! Tool execution display widget.
//!
//! Shows currently running tools with their output in a collapsible region.
//! Uses animated braille spinners, tool-type color coding, and elapsed time
//! tracking — mirrors the Python `DefaultToolRenderer`.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::app::ToolExecution;
use crate::formatters::style_tokens;
use crate::formatters::tool_registry::{categorize_tool, tool_color};
use crate::widgets::spinner::{COMPLETED_CHAR, SPINNER_FRAMES};

/// Widget that displays active tool executions.
pub struct ToolDisplayWidget<'a> {
    tools: &'a [ToolExecution],
}

impl<'a> ToolDisplayWidget<'a> {
    pub fn new(tools: &'a [ToolExecution]) -> Self {
        Self { tools }
    }
}

impl Widget for ToolDisplayWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(style_tokens::BORDER))
            .title(Span::styled(
                " Tools ",
                Style::default()
                    .fg(style_tokens::BLUE_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ));

        let mut lines: Vec<Line> = Vec::new();

        for tool in self.tools {
            // Determine the tool category color
            let category = categorize_tool(&tool.name);
            let name_color = tool_color(category);

            // Spinner / status indicator
            let (spinner_str, spinner_color) = if tool.is_finished() {
                if tool.is_success() {
                    (COMPLETED_CHAR.to_string(), style_tokens::SUCCESS)
                } else {
                    (COMPLETED_CHAR.to_string(), style_tokens::ERROR)
                }
            } else {
                // Animated braille spinner
                let frame_idx = tool.tick_count % SPINNER_FRAMES.len();
                (
                    SPINNER_FRAMES[frame_idx].to_string(),
                    style_tokens::BLUE_BRIGHT,
                )
            };

            // Tool header with elapsed time
            let elapsed_str = format!(" ({}s)", tool.elapsed_secs);

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {spinner_str} "),
                    Style::default().fg(spinner_color),
                ),
                Span::styled(
                    tool.name.clone(),
                    Style::default().fg(name_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(elapsed_str, Style::default().fg(style_tokens::GREY)),
            ]));

            // Tree indent for nested tools
            let indent = if tool.depth > 0 {
                "  ".repeat(tool.depth) + "\u{2514}\u{2500} "
            } else {
                String::new()
            };

            // Last few output lines (max 4)
            let start = tool.output_lines.len().saturating_sub(4);
            for (i, line) in tool.output_lines[start..].iter().enumerate() {
                let prefix = if i == 0 {
                    format!("  \u{23bf}  {indent}{line}")
                } else {
                    format!("     {indent}{line}")
                };
                lines.push(Line::from(Span::styled(
                    prefix,
                    Style::default().fg(style_tokens::SUBTLE),
                )));
            }
        }

        if lines.is_empty() {
            return;
        }

        let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });

        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolState;
    use std::time::Instant;

    #[test]
    fn test_empty_tool_display() {
        let tools: Vec<ToolExecution> = vec![];
        let _widget = ToolDisplayWidget::new(&tools);
    }

    #[test]
    fn test_tool_display_with_output() {
        let tools = vec![ToolExecution {
            id: "t1".into(),
            name: "bash".into(),
            output_lines: vec!["file1.rs".into(), "file2.rs".into()],
            state: ToolState::Running,
            elapsed_secs: 3,
            started_at: Instant::now(),
            tick_count: 0,
            parent_id: None,
            depth: 0,
            args: Default::default(),
        }];
        let _widget = ToolDisplayWidget::new(&tools);
    }

    #[test]
    fn test_tool_display_nested() {
        let tools = vec![
            ToolExecution {
                id: "t1".into(),
                name: "spawn_subagent".into(),
                output_lines: vec![],
                state: ToolState::Running,
                elapsed_secs: 5,
                started_at: Instant::now(),
                tick_count: 0,
                parent_id: None,
                depth: 0,
                args: Default::default(),
            },
            ToolExecution {
                id: "t2".into(),
                name: "read_file".into(),
                output_lines: vec!["reading...".into()],
                state: ToolState::Running,
                elapsed_secs: 2,
                started_at: Instant::now(),
                tick_count: 0,
                parent_id: Some("t1".into()),
                depth: 1,
                args: Default::default(),
            },
        ];
        let _widget = ToolDisplayWidget::new(&tools);
    }
}
