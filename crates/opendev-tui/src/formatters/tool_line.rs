//! Shared tool line construction for TUI widgets.
//!
//! Eliminates duplicated `Span` vector assembly across spinner, nested_tool,
//! tool_display, and tool_format modules.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::style_tokens;
use crate::widgets::spinner::{COMPLETED_CHAR, FAILURE_CHAR};

/// Controls verb/arg/elapsed coloring.
pub enum ToolLineStyle {
    /// Top-level tool: PRIMARY+BOLD verb, SUBTLE arg, GREY elapsed.
    Primary,
    /// Nested/subagent tool: SUBTLE verb, GREY arg, SUBTLE elapsed.
    Nested,
}

/// Format elapsed seconds consistently: `"0s"`, `"5s"`, `"1m 5s"`.
pub fn format_elapsed(secs: u64) -> String {
    if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

/// Format a token count as human-readable (e.g., `"500 tokens"`, `"1.5k tokens"`, `"3.5M tokens"`).
pub fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M tokens", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k tokens", tokens as f64 / 1_000.0)
    } else {
        format!("{tokens} tokens")
    }
}

/// Build a `Line` for an **active** (spinning) tool.
///
/// Layout: `{prefix_spans}{spinner} {verb} {arg} {elapsed}`
pub fn tool_line_active(
    prefix_spans: Vec<Span<'static>>,
    spinner_char: char,
    verb: String,
    arg: String,
    elapsed: Option<String>,
    style: ToolLineStyle,
) -> Line<'static> {
    let mut spans = prefix_spans;

    spans.push(Span::styled(
        format!("{spinner_char} "),
        Style::default().fg(style_tokens::BLUE_BRIGHT),
    ));

    let (verb_style, arg_style, elapsed_style) = match style {
        ToolLineStyle::Primary => (
            Style::default()
                .fg(style_tokens::PRIMARY)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(style_tokens::SUBTLE),
            Style::default().fg(style_tokens::GREY),
        ),
        ToolLineStyle::Nested => (
            Style::default().fg(style_tokens::SUBTLE),
            Style::default().fg(style_tokens::GREY),
            Style::default().fg(style_tokens::SUBTLE),
        ),
    };

    spans.push(Span::styled(verb, verb_style));
    spans.push(Span::styled(format!(" {arg}"), arg_style));

    if let Some(el) = elapsed {
        spans.push(Span::styled(format!(" {el}"), elapsed_style));
    }

    Line::from(spans)
}

/// Build a `Line` for a **completed** tool.
///
/// Layout: `{prefix_spans}{icon} {verb} {arg} {elapsed}`
pub fn tool_line_completed(
    prefix_spans: Vec<Span<'static>>,
    success: bool,
    verb: String,
    arg: String,
    elapsed: Option<String>,
    style: ToolLineStyle,
) -> Line<'static> {
    let mut spans = prefix_spans;

    let (icon, icon_color) = if success {
        (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
    } else {
        (FAILURE_CHAR, style_tokens::ERROR)
    };

    spans.push(Span::styled(
        format!("{icon} "),
        Style::default().fg(icon_color),
    ));

    let (verb_style, arg_style, elapsed_style) = match style {
        ToolLineStyle::Primary => (
            Style::default()
                .fg(style_tokens::PRIMARY)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(style_tokens::SUBTLE),
            Style::default().fg(style_tokens::GREY),
        ),
        ToolLineStyle::Nested => (
            Style::default().fg(style_tokens::SUBTLE),
            Style::default().fg(style_tokens::GREY),
            Style::default().fg(style_tokens::SUBTLE),
        ),
    };

    spans.push(Span::styled(verb, verb_style));
    spans.push(Span::styled(format!(" {arg}"), arg_style));

    if let Some(el) = elapsed {
        spans.push(Span::styled(format!(" {el}"), elapsed_style));
    }

    Line::from(spans)
}

#[cfg(test)]
#[path = "tool_line_tests.rs"]
mod tests;
