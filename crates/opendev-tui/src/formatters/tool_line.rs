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
mod tests {
    use super::*;

    #[test]
    fn test_format_elapsed_zero() {
        assert_eq!(format_elapsed(0), "0s");
    }

    #[test]
    fn test_format_elapsed_seconds_only() {
        assert_eq!(format_elapsed(5), "5s");
        assert_eq!(format_elapsed(59), "59s");
    }

    #[test]
    fn test_format_elapsed_with_minutes() {
        assert_eq!(format_elapsed(60), "1m 0s");
        assert_eq!(format_elapsed(65), "1m 5s");
        assert_eq!(format_elapsed(125), "2m 5s");
    }

    #[test]
    fn test_tool_line_active_primary_span_count() {
        let line = tool_line_active(
            vec![],
            '\u{2800}',
            "Reading".into(),
            "foo.rs".into(),
            Some("5s".into()),
            ToolLineStyle::Primary,
        );
        // spinner, verb, arg, elapsed = 4 spans
        assert_eq!(line.spans.len(), 4);
    }

    #[test]
    fn test_tool_line_active_nested_no_elapsed() {
        let line = tool_line_active(
            vec![Span::raw("  \u{23bf}  ")],
            '\u{2800}',
            "Reading".into(),
            "foo.rs".into(),
            None,
            ToolLineStyle::Nested,
        );
        // prefix + spinner + verb + arg = 4 spans (no elapsed)
        assert_eq!(line.spans.len(), 4);
    }

    #[test]
    fn test_tool_line_completed_success_icon() {
        let line = tool_line_completed(
            vec![],
            true,
            "Read".into(),
            "foo.rs".into(),
            None,
            ToolLineStyle::Primary,
        );
        let icon_span = &line.spans[0];
        assert!(icon_span.content.contains(COMPLETED_CHAR));
        assert_eq!(icon_span.style.fg, Some(style_tokens::GREEN_BRIGHT));
    }

    #[test]
    fn test_tool_line_completed_failure_icon() {
        let line = tool_line_completed(
            vec![],
            false,
            "Bash".into(),
            "ls".into(),
            None,
            ToolLineStyle::Primary,
        );
        let icon_span = &line.spans[0];
        assert!(icon_span.content.contains(FAILURE_CHAR));
        assert_eq!(icon_span.style.fg, Some(style_tokens::ERROR));
    }

    #[test]
    fn test_tool_line_active_primary_colors() {
        let line = tool_line_active(
            vec![],
            '\u{2800}',
            "Writing".into(),
            "bar.rs".into(),
            Some("10s".into()),
            ToolLineStyle::Primary,
        );
        // verb span (index 1) should be PRIMARY + BOLD
        assert_eq!(line.spans[1].style.fg, Some(style_tokens::PRIMARY));
        assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD));
        // arg span (index 2) should be SUBTLE
        assert_eq!(line.spans[2].style.fg, Some(style_tokens::SUBTLE));
        // elapsed span (index 3) should be GREY
        assert_eq!(line.spans[3].style.fg, Some(style_tokens::GREY));
    }

    #[test]
    fn test_tool_line_active_nested_colors() {
        let line = tool_line_active(
            vec![],
            '\u{2800}',
            "Reading".into(),
            "baz.rs".into(),
            Some("3s".into()),
            ToolLineStyle::Nested,
        );
        // verb span should be SUBTLE
        assert_eq!(line.spans[1].style.fg, Some(style_tokens::SUBTLE));
        // arg span should be GREY
        assert_eq!(line.spans[2].style.fg, Some(style_tokens::GREY));
        // elapsed span should be SUBTLE
        assert_eq!(line.spans[3].style.fg, Some(style_tokens::SUBTLE));
    }
}
