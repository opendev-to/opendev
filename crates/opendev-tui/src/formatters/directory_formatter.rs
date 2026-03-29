//! Directory/search result formatter.
//!
//! Formats Glob and Grep tool output as file lists with counts.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::base::{FormattedOutput, ToolFormatter};
use super::style_tokens;

/// Formatter for Glob/Grep search results.
pub struct DirectoryFormatter;

/// Maximum number of result lines to display before truncating.
const MAX_RESULTS: usize = 40;

impl ToolFormatter for DirectoryFormatter {
    fn format<'a>(&self, tool_name: &str, output: &str) -> FormattedOutput<'a> {
        let label = match tool_name {
            "Glob" | "list_files" => "matching files",
            "Grep" | "search" => "matching results",
            _ => "results",
        };

        let all_lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
        let total = all_lines.len();

        let header = Line::from(vec![
            Span::styled("  🔍 ".to_string(), Style::default().fg(style_tokens::CYAN)),
            Span::styled(
                format!("{total} {label}"),
                Style::default().fg(style_tokens::CYAN),
            ),
        ]);

        let display_count = total.min(MAX_RESULTS);
        let body: Vec<Line<'a>> = all_lines[..display_count]
            .iter()
            .map(|line| {
                Line::from(vec![
                    Span::styled("    ".to_string(), Style::default()),
                    Span::styled(line.to_string(), Style::default().fg(style_tokens::PRIMARY)),
                ])
            })
            .collect();

        let footer = if total > MAX_RESULTS {
            let remaining = total - MAX_RESULTS;
            Some(Line::from(Span::styled(
                format!("  ... and {remaining} more"),
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

    fn handles(&self, tool_name: &str) -> bool {
        matches!(tool_name, "Glob" | "Grep" | "list_files" | "search")
    }
}

#[cfg(test)]
#[path = "directory_formatter_tests.rs"]
mod tests;
