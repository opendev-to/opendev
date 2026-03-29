//! File operation output formatter.
//!
//! Handles Read (shows content with line numbers), Write/Edit (shows diff
//! with +/- coloring).

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::base::{FormattedOutput, ToolFormatter, truncate_lines};
use super::style_tokens;

/// Formatter for file read/write/edit tool output.
pub struct FileFormatter;

/// Maximum lines to show before truncating.
const MAX_DISPLAY_LINES: usize = 50;

impl FileFormatter {
    /// Format file content with line numbers (for Read output).
    fn format_read<'a>(output: &str) -> FormattedOutput<'a> {
        let truncated = truncate_lines(output, MAX_DISPLAY_LINES);
        let total_lines = output.lines().count();

        let header = Line::from(vec![
            Span::styled(
                "  📄 ".to_string(),
                Style::default().fg(style_tokens::BLUE_PATH),
            ),
            Span::styled(
                format!("File content ({total_lines} lines)"),
                Style::default().fg(style_tokens::BLUE_PATH),
            ),
        ]);

        let body: Vec<Line<'a>> = truncated
            .lines()
            .enumerate()
            .map(|(i, line)| {
                Line::from(vec![
                    Span::styled(
                        format!("  {:<4} ", i + 1),
                        Style::default().fg(style_tokens::GREY),
                    ),
                    Span::raw(line.to_string()),
                ])
            })
            .collect();

        let footer = if total_lines > MAX_DISPLAY_LINES {
            Some(Line::from(Span::styled(
                format!("  ... {total_lines} total lines"),
                Style::default().fg(style_tokens::SUBTLE),
            )))
        } else {
            None
        };

        FormattedOutput {
            header,
            body,
            footer,
        }
    }

    /// Format diff output (for Write/Edit).
    fn format_diff<'a>(tool_name: &str, output: &str) -> FormattedOutput<'a> {
        let verb = if tool_name == "Write" || tool_name == "write_file" {
            "Written"
        } else {
            "Edited"
        };

        let header = Line::from(vec![
            Span::styled(
                "  ✎ ".to_string(),
                Style::default().fg(style_tokens::SUCCESS),
            ),
            Span::styled(
                format!("{verb} file"),
                Style::default().fg(style_tokens::SUCCESS),
            ),
        ]);

        let mut additions = 0usize;
        let mut removals = 0usize;

        let body: Vec<Line<'a>> = output
            .lines()
            .map(|line| {
                if let Some(rest) = line.strip_prefix('+') {
                    additions += 1;
                    Line::from(Span::styled(
                        format!("    +{rest}"),
                        Style::default().fg(style_tokens::SUCCESS),
                    ))
                } else if let Some(rest) = line.strip_prefix('-') {
                    removals += 1;
                    Line::from(Span::styled(
                        format!("    -{rest}"),
                        Style::default().fg(style_tokens::ERROR),
                    ))
                } else {
                    Line::from(Span::raw(format!("    {line}")))
                }
            })
            .collect();

        let footer = Some(Line::from(vec![
            Span::styled(
                format!("  +{additions} ",),
                Style::default().fg(style_tokens::SUCCESS),
            ),
            Span::styled(
                format!("-{removals}"),
                Style::default().fg(style_tokens::ERROR),
            ),
        ]));

        FormattedOutput {
            header,
            body,
            footer,
        }
    }
}

impl ToolFormatter for FileFormatter {
    fn format<'a>(&self, tool_name: &str, output: &str) -> FormattedOutput<'a> {
        match tool_name {
            "Read" | "read_file" | "read_pdf" => Self::format_read(output),
            _ => Self::format_diff(tool_name, output),
        }
    }

    fn handles(&self, tool_name: &str) -> bool {
        matches!(
            tool_name,
            "Read"
                | "Write"
                | "Edit"
                | "read_file"
                | "write_file"
                | "edit_file"
                | "read_pdf"
                | "patch_file"
        )
    }
}

#[cfg(test)]
#[path = "file_formatter_tests.rs"]
mod tests;
