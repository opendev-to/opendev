//! Nested tool display widget for subagent progress.
//!
//! Renders a tree-structured view of subagent tool calls,
//! showing which subagent is running, its active tool calls,
//! and completion status with tree connectors (similar to the
//! Python `NestedToolMixin`).

mod state;

pub use state::{CompletedToolCall, NestedToolCallState, SubagentDisplayState};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::formatters::style_tokens;
use crate::formatters::tool_line::{
    ToolLineStyle, format_elapsed, tool_line_active, tool_line_completed,
};
use crate::formatters::tool_registry::format_tool_call_parts_short;
use crate::widgets::spinner::{
    FAILURE_CHAR, SPINNER_FRAMES, SUCCESS_CHAR, TREE_BRANCH, TREE_LAST, TREE_VERTICAL,
};

/// Widget that renders the nested subagent tool display.
pub struct NestedToolWidget<'a> {
    subagents: &'a [SubagentDisplayState],
    working_dir: Option<&'a str>,
    shortener: Option<&'a crate::formatters::PathShortener>,
}

impl<'a> NestedToolWidget<'a> {
    pub fn new(subagents: &'a [SubagentDisplayState]) -> Self {
        Self {
            subagents,
            working_dir: None,
            shortener: None,
        }
    }

    pub fn working_dir(mut self, wd: &'a str) -> Self {
        self.working_dir = Some(wd);
        self
    }

    pub fn path_shortener(mut self, shortener: &'a crate::formatters::PathShortener) -> Self {
        self.shortener = Some(shortener);
        self
    }
}

impl Widget for NestedToolWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.subagents.is_empty() {
            return;
        }

        let owned_shortener;
        let shortener = if let Some(s) = self.shortener {
            s
        } else {
            owned_shortener = crate::formatters::PathShortener::new(self.working_dir);
            &owned_shortener
        };

        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(style_tokens::BORDER))
            .title(Span::styled(
                " Subagents ",
                Style::default()
                    .fg(style_tokens::HEADING_1)
                    .add_modifier(Modifier::BOLD),
            ));

        let mut lines: Vec<Line> = Vec::new();

        for (i, subagent) in self.subagents.iter().enumerate() {
            let is_last = i == self.subagents.len() - 1;

            // Subagent header line
            let connector = if is_last { TREE_LAST } else { TREE_BRANCH };
            let (status_str, status_color) = if subagent.finished {
                if subagent.success {
                    (SUCCESS_CHAR.to_string(), style_tokens::SUCCESS)
                } else {
                    (FAILURE_CHAR.to_string(), style_tokens::ERROR)
                }
            } else {
                let slow_tick = subagent.tick / 3;
                let spinner_idx = slow_tick % SPINNER_FRAMES.len();
                (
                    SPINNER_FRAMES[spinner_idx].to_string(),
                    style_tokens::BLUE_BRIGHT,
                )
            };

            let elapsed = subagent.elapsed_secs();
            let task_text = shortener.shorten_text(&subagent.task);
            let task_preview = if task_text.len() > 60 {
                format!("{}...", &task_text[..60])
            } else {
                task_text
            };

            let elapsed_str = format_elapsed(elapsed);

            // Format token count
            let token_str = if subagent.token_count > 0 {
                let k = subagent.token_count as f64 / 1000.0;
                format!(" \u{00b7} {k:.1}k tokens")
            } else {
                String::new()
            };

            // Build stats suffix
            let stats = format!(
                " ({} tool uses{} \u{00b7} {})",
                subagent.tool_call_count, token_str, elapsed_str
            );

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {connector} "),
                    Style::default().fg(style_tokens::SUBTLE),
                ),
                Span::styled(format!("{status_str} "), Style::default().fg(status_color)),
                Span::styled(
                    subagent.name.clone(),
                    Style::default()
                        .fg(style_tokens::CYAN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(": {task_preview}"),
                    Style::default().fg(style_tokens::SUBTLE),
                ),
                Span::styled(stats, Style::default().fg(style_tokens::SUBTLE)),
            ]));

            // Show active tool calls
            let vertical = if is_last {
                "   "
            } else {
                &format!(" {TREE_VERTICAL}  ")
            };
            let active_count = subagent.active_tools.len();

            for (j, tool_state) in subagent.active_tools.values().enumerate() {
                let tool_is_last = j == active_count - 1 && subagent.completed_tools.is_empty();
                let tool_connector = if tool_is_last { TREE_LAST } else { TREE_BRANCH };
                let slow_tick = tool_state.tick / 3;
                let spinner_idx = slow_tick % SPINNER_FRAMES.len();
                let spinner_ch = SPINNER_FRAMES[spinner_idx];
                let tool_elapsed = tool_state.started_at.elapsed().as_secs();
                let (verb, arg) = format_tool_call_parts_short(
                    &tool_state.tool_name,
                    &tool_state.args,
                    shortener,
                );

                let tree_prefix = vec![Span::styled(
                    format!("  {vertical}{tool_connector} "),
                    Style::default().fg(style_tokens::SUBTLE),
                )];
                lines.push(tool_line_active(
                    tree_prefix,
                    spinner_ch,
                    verb,
                    arg,
                    Some(format!("({})", format_elapsed(tool_elapsed))),
                    ToolLineStyle::Nested,
                ));
            }

            // Show last few completed tools (max 3)
            // Use actual completed_tools len for slicing (it's capped at 100)
            let completed_start = subagent.completed_tools.len().saturating_sub(3);
            let visible_completed = &subagent.completed_tools[completed_start..];
            for (j, completed) in visible_completed.iter().enumerate() {
                let is_last_tool = j == visible_completed.len() - 1;
                let tool_connector = if is_last_tool { TREE_LAST } else { TREE_BRANCH };
                let (verb, arg) =
                    format_tool_call_parts_short(&completed.tool_name, &completed.args, shortener);

                let tree_prefix = vec![Span::styled(
                    format!("  {vertical}{tool_connector} "),
                    Style::default().fg(style_tokens::SUBTLE),
                )];
                lines.push(tool_line_completed(
                    tree_prefix,
                    completed.success,
                    verb,
                    arg,
                    Some(format!("({})", format_elapsed(completed.elapsed.as_secs()))),
                    ToolLineStyle::Nested,
                ));
            }

            // Show hidden count if there are more completed tools
            // Use tool_call_count (actual total) since completed_tools is capped at 100
            let total_completed = subagent
                .tool_call_count
                .saturating_sub(subagent.active_tools.len());
            let visible_count = visible_completed.len();
            let hidden_count = total_completed.saturating_sub(visible_count);
            if hidden_count > 0 {
                lines.push(Line::from(Span::styled(
                    format!("  {vertical}   +{hidden_count} more tool uses (ctrl+b to run in background)"),
                    Style::default()
                        .fg(style_tokens::SUBTLE)
                        .add_modifier(Modifier::ITALIC),
                )));
            }

            // Show shallow warning if present
            if let Some(ref warning) = subagent.shallow_warning {
                lines.push(Line::from(Span::styled(
                    format!("  {vertical}   {warning}"),
                    Style::default().fg(style_tokens::WARNING),
                )));
            }
        }

        let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });

        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests;
