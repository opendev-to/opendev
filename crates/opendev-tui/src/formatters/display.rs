//! Display formatting utilities.
//!
//! Mirrors Python `DisplayFormatter` — handles system-reminder tag filtering,
//! error/warning/info formatting, and content sanitization for display.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::style_tokens::{self, RESULT_PREFIX};

/// Strip `<system-reminder>` XML tags and their content from display text.
///
/// System reminders are injected into messages for the LLM but should not
/// be shown to the user in the conversation view.
pub fn strip_system_reminders(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find("<system-reminder>") {
            // Add everything before the tag
            result.push_str(&remaining[..start]);

            // Find the closing tag
            let after_open = &remaining[start..];
            if let Some(end) = after_open.find("</system-reminder>") {
                let close_tag_len = "</system-reminder>".len();
                remaining = &after_open[end + close_tag_len..];
            } else {
                // No closing tag — skip rest (malformed)
                break;
            }
        } else {
            result.push_str(remaining);
            break;
        }
    }

    // Collapse runs of 2+ newlines (left over from removal) into a single newline
    let mut cleaned = result.clone();
    while cleaned.contains("\n\n") {
        cleaned = cleaned.replace("\n\n", "\n");
    }

    cleaned.trim().to_string()
}

/// Format an error message for display.
pub fn format_error<'a>(primary: &str, secondary: Option<&str>) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            RESULT_PREFIX.to_string(),
            Style::default().fg(style_tokens::ERROR),
        ),
        Span::styled(
            primary.to_string(),
            Style::default().fg(style_tokens::ERROR),
        ),
    ]));

    if let Some(sec) = secondary {
        lines.push(Line::from(vec![
            Span::styled(
                RESULT_PREFIX.to_string(),
                Style::default().fg(style_tokens::SUBTLE),
            ),
            Span::styled(sec.to_string(), Style::default().fg(style_tokens::SUBTLE)),
        ]));
    }

    lines
}

/// Format a warning message for display.
pub fn format_warning<'a>(primary: &str, secondary: Option<&str>) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            RESULT_PREFIX.to_string(),
            Style::default().fg(style_tokens::WARNING),
        ),
        Span::styled(
            primary.to_string(),
            Style::default().fg(style_tokens::WARNING),
        ),
    ]));

    if let Some(sec) = secondary {
        lines.push(Line::from(vec![
            Span::styled(
                RESULT_PREFIX.to_string(),
                Style::default().fg(style_tokens::SUBTLE),
            ),
            Span::styled(sec.to_string(), Style::default().fg(style_tokens::SUBTLE)),
        ]));
    }

    lines
}

/// Format an info message for display.
pub fn format_info<'a>(primary: &str, secondary: Option<&str>) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            RESULT_PREFIX.to_string(),
            Style::default().fg(style_tokens::GREY),
        ),
        Span::raw(primary.to_string()),
    ]));

    if let Some(sec) = secondary {
        lines.push(Line::from(vec![
            Span::styled(
                RESULT_PREFIX.to_string(),
                Style::default().fg(style_tokens::SUBTLE),
            ),
            Span::styled(sec.to_string(), Style::default().fg(style_tokens::SUBTLE)),
        ]));
    }

    lines
}

/// Truncate output text with head/tail mode.
///
/// Returns `(truncated_text, was_truncated, hidden_count)`.
pub fn truncate_output(text: &str, head: usize, tail: usize) -> (String, bool, usize) {
    if text.is_empty() {
        return (String::new(), false, 0);
    }

    let all_lines: Vec<&str> = text.lines().collect();
    let total = head + tail;

    if all_lines.len() <= total {
        return (text.to_string(), false, 0);
    }

    let hidden = all_lines.len() - total;
    let head_part: Vec<&str> = all_lines[..head].to_vec();
    let tail_part: Vec<&str> = if tail > 0 {
        all_lines[all_lines.len() - tail..].to_vec()
    } else {
        vec![]
    };

    let mut result = head_part.join("\n");
    result.push_str(&format!("\n... {hidden} lines hidden ...\n"));
    result.push_str(&tail_part.join("\n"));

    (result, true, hidden)
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;
