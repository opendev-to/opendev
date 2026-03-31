//! Tool call formatting helpers for conversation display.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::app::DisplayToolCall;
use crate::formatters::style_tokens;
use crate::formatters::tool_line::{ToolLineStyle, tool_line_completed};
use crate::formatters::tool_registry::format_tool_call_parts_with_wd;
use crate::widgets::spinner::{COMPLETED_CHAR, CONTINUATION_CHAR};

/// Format a tool call as a styled line with category color coding.
pub(crate) fn format_tool_call(tc: &DisplayToolCall, working_dir: Option<&str>) -> Line<'static> {
    // For ask_user, display as "⏺ User answered Claude's questions:"
    if tc.name == "ask_user" {
        let (icon, icon_color) = if tc.success {
            (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
        } else {
            (COMPLETED_CHAR, style_tokens::ERROR)
        };
        return Line::from(vec![
            Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
            Span::styled(
                "User answered Claude's questions:",
                Style::default()
                    .fg(style_tokens::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }

    let (verb, arg) = format_tool_call_parts_with_wd(&tc.name, &tc.arguments, working_dir);

    tool_line_completed(vec![], tc.success, verb, arg, None, ToolLineStyle::Primary)
}

/// Format a nested tool call with indentation.
///
/// At depth 0, shows the `⎿` continuation character. At depth 1+, uses
/// spaces of equal width to avoid visually broken `⎿⎿` double-nesting.
pub(crate) fn format_nested_tool_call(
    tc: &DisplayToolCall,
    depth: usize,
    working_dir: Option<&str>,
) -> Line<'static> {
    let (verb, arg) = format_tool_call_parts_with_wd(&tc.name, &tc.arguments, working_dir);

    let continuation_prefix = if depth == 0 {
        vec![Span::styled(
            format!("  {CONTINUATION_CHAR}  "),
            Style::default().fg(style_tokens::GREY),
        )]
    } else {
        // Same visual width as "  ⎿  " (5 chars) but spaces only
        vec![Span::raw("     ")]
    };

    tool_line_completed(
        continuation_prefix,
        tc.success,
        verb,
        arg,
        None,
        ToolLineStyle::Nested,
    )
}
