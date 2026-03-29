//! Bash tool output formatter.
//!
//! Formats command execution results with exit code coloring:
//! green for success (exit 0), red for failure (nonzero).

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::base::{FormattedOutput, ToolFormatter};
use super::style_tokens;

/// Formatter for Bash/command execution tool output.
pub struct BashFormatter;

impl BashFormatter {
    /// Parse exit code from output text.
    ///
    /// Looks for a trailing line like `Exit code: 0` or `exit_code: 1`.
    fn parse_exit_code(output: &str) -> Option<i32> {
        for line in output.lines().rev() {
            let trimmed = line.trim().to_lowercase();
            if let Some(rest) = trimmed.strip_prefix("exit code:")
                && let Ok(code) = rest.trim().parse::<i32>()
            {
                return Some(code);
            }
            if let Some(rest) = trimmed.strip_prefix("exit_code:")
                && let Ok(code) = rest.trim().parse::<i32>()
            {
                return Some(code);
            }
        }
        None
    }

    /// Extract command line from the output (first line if it looks like a command).
    fn extract_command(output: &str) -> Option<&str> {
        let first = output.lines().next()?;
        let trimmed = first.trim();
        if trimmed.starts_with('$') || trimmed.starts_with('>') {
            Some(trimmed.trim_start_matches(['$', '>']).trim())
        } else {
            None
        }
    }
}

impl ToolFormatter for BashFormatter {
    fn format<'a>(&self, _tool_name: &str, output: &str) -> FormattedOutput<'a> {
        let exit_code = Self::parse_exit_code(output);
        let status_color = style_tokens::GREY;

        // Header: command line or generic label
        let cmd = Self::extract_command(output);
        let header = Line::from(vec![
            Span::styled("  $ ".to_string(), Style::default().fg(style_tokens::GREY)),
            Span::styled(
                cmd.unwrap_or("command").to_string(),
                Style::default().fg(style_tokens::WARNING),
            ),
        ]);

        // Body: all output lines except the command prefix and exit code trailer
        let lines: Vec<&str> = output.lines().collect();
        let start = if cmd.is_some() { 1 } else { 0 };
        let end = if exit_code.is_some() && lines.len() > start {
            // Skip trailing exit code line(s)
            let mut e = lines.len();
            for i in (start..lines.len()).rev() {
                let t = lines[i].trim().to_lowercase();
                if t.starts_with("exit code:") || t.starts_with("exit_code:") {
                    e = i;
                    break;
                }
            }
            e
        } else {
            lines.len()
        };

        let body: Vec<Line<'a>> = lines[start..end]
            .iter()
            .map(|l| Line::from(Span::raw(format!("    {l}"))))
            .collect();

        // Footer: exit code
        let footer = exit_code.map(|code| {
            Line::from(vec![
                Span::styled(
                    "  ─ exit ".to_string(),
                    Style::default().fg(style_tokens::GREY),
                ),
                Span::styled(code.to_string(), Style::default().fg(status_color)),
            ])
        });

        FormattedOutput {
            header,
            body,
            footer,
        }
    }

    fn handles(&self, tool_name: &str) -> bool {
        matches!(tool_name, "Bash" | "run_command" | "bash_execute")
    }
}

#[cfg(test)]
#[path = "bash_formatter_tests.rs"]
mod tests;
