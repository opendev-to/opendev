//! Generic fallback formatter.
//!
//! Attempts JSON pretty-print; otherwise shows plain text with truncation.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::base::{FormattedOutput, ToolFormatter, truncate_lines};
use super::style_tokens;

/// Fallback formatter for any tool not handled by a specific formatter.
pub struct GenericFormatter;

/// Maximum lines before truncation.
const MAX_LINES: usize = 60;

impl ToolFormatter for GenericFormatter {
    fn format<'a>(&self, tool_name: &str, output: &str) -> FormattedOutput<'a> {
        let header = Line::from(vec![
            Span::styled(
                "  ⚙ ".to_string(),
                Style::default().fg(style_tokens::PRIMARY),
            ),
            Span::styled(
                tool_name.to_string(),
                Style::default().fg(style_tokens::PRIMARY),
            ),
        ]);

        // Try to pretty-print as JSON
        let display_text = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(output) {
            match serde_json::to_string_pretty(&parsed) {
                Ok(pretty) => pretty,
                Err(_) => output.to_string(),
            }
        } else {
            output.to_string()
        };

        let truncated = truncate_lines(&display_text, MAX_LINES);
        let total = display_text.lines().count();

        let body: Vec<Line<'a>> = truncated
            .lines()
            .map(|line| Line::from(Span::raw(format!("    {line}"))))
            .collect();

        let footer = if total > MAX_LINES {
            Some(Line::from(Span::styled(
                format!("  ... {total} total lines (truncated)"),
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

    fn handles(&self, _tool_name: &str) -> bool {
        // Generic formatter handles everything as a fallback.
        true
    }
}

#[cfg(test)]
#[path = "generic_formatter_tests.rs"]
mod tests;
