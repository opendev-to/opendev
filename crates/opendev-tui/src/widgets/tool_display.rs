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
use crate::formatters::tool_line::{
    ToolLineStyle, format_elapsed, tool_line_active, tool_line_completed,
};
use crate::formatters::tool_registry::format_tool_call_parts_short;
use crate::widgets::spinner::SPINNER_FRAMES;

/// Widget that displays active tool executions.
pub struct ToolDisplayWidget<'a> {
    tools: &'a [ToolExecution],
    working_dir: Option<&'a str>,
}

impl<'a> ToolDisplayWidget<'a> {
    pub fn new(tools: &'a [ToolExecution]) -> Self {
        Self {
            tools,
            working_dir: None,
        }
    }

    pub fn working_dir(mut self, wd: &'a str) -> Self {
        self.working_dir = Some(wd);
        self
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

        let shortener = crate::formatters::PathShortener::new(self.working_dir);
        let mut lines: Vec<Line> = Vec::new();

        for tool in self.tools {
            // Tool header with elapsed time
            let (verb, arg) = format_tool_call_parts_short(&tool.name, &tool.args, &shortener);
            let indent_prefix = vec![Span::raw("  ")];
            let elapsed = Some(format!("({})", format_elapsed(tool.elapsed_secs)));

            if tool.is_finished() {
                lines.push(tool_line_completed(
                    indent_prefix,
                    tool.is_success(),
                    verb,
                    arg,
                    elapsed,
                    ToolLineStyle::Primary,
                ));
            } else {
                let frame_idx = tool.tick_count % SPINNER_FRAMES.len();
                let spinner_ch = SPINNER_FRAMES[frame_idx];
                lines.push(tool_line_active(
                    indent_prefix,
                    spinner_ch,
                    verb,
                    arg,
                    elapsed,
                    ToolLineStyle::Primary,
                ));
            }

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
