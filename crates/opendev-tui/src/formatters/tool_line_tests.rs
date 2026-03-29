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
