use super::*;

#[test]
fn test_short_line_no_wrap() {
    let md_lines = vec![Line::from(vec![Span::raw("Hello world")])];
    let first = vec![Span::raw("* ")];
    let cont = vec![Span::raw("  ")];

    let result = wrap_spans_to_lines(md_lines, first, cont, 80);
    assert_eq!(result.len(), 1);
    let text: String = result[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "* Hello world");
}

#[test]
fn test_wraps_long_line() {
    let long = "word ".repeat(20).trim().to_string(); // ~99 chars
    let md_lines = vec![Line::from(vec![Span::raw(long)])];
    let first = vec![Span::raw("* ")];
    let cont = vec![Span::raw("  ")];

    let result = wrap_spans_to_lines(md_lines, first, cont, 40);
    assert!(result.len() > 1, "should have wrapped into multiple lines");

    // First line starts with "* "
    assert!(result[0].spans[0].content.as_ref().starts_with("* "));
    // Continuation lines start with "  "
    for line in &result[1..] {
        assert_eq!(line.spans[0].content.as_ref(), "  ");
    }

    // All lines should fit within 40 chars
    for line in &result {
        let w: usize = line.spans.iter().map(|s| span_width(s)).sum();
        assert!(w <= 40, "line width {w} exceeds max 40");
    }
}

#[test]
fn test_blank_lines_preserved() {
    let md_lines = vec![
        Line::from(vec![Span::raw("Hello")]),
        Line::from(vec![Span::raw("")]),
        Line::from(vec![Span::raw("World")]),
    ];
    let first = vec![Span::raw("* ")];
    let cont = vec![Span::raw("  ")];

    let result = wrap_spans_to_lines(md_lines, first, cont, 80);
    assert_eq!(result.len(), 3);
}

#[test]
fn test_code_line_not_wrapped() {
    let code_style = Style::default().bg(CODE_BG);
    let long_code = "x".repeat(200);
    let md_lines = vec![Line::from(vec![Span::styled(
        long_code.clone(),
        code_style,
    )])];
    let first = vec![Span::raw("* ")];
    let cont = vec![Span::raw("  ")];

    let result = wrap_spans_to_lines(md_lines, first, cont, 40);
    // Code line should NOT be wrapped — should be 1 line
    assert_eq!(result.len(), 1);
}

#[test]
fn test_thinking_cont_prefix() {
    use super::super::style_tokens::{Indent, THINKING_BG};
    let md_lines = vec![
        Line::from(vec![Span::raw("First line of thinking")]),
        Line::from(vec![Span::raw("Second line of thinking")]),
    ];
    let first = vec![Span::styled("⟡ ", Style::default().fg(THINKING_BG))];
    let cont = vec![Span::styled(
        Indent::THINKING_CONT,
        Style::default().fg(THINKING_BG),
    )];

    let result = wrap_spans_to_lines(md_lines, first, cont, 80);
    assert_eq!(result.len(), 2);
    // First line: ⟡ prefix
    assert!(result[0].spans[0].content.as_ref().starts_with('⟡'));
    // Second line: │ prefix
    assert!(result[1].spans[0].content.as_ref().starts_with('│'));
}

#[test]
fn test_reasoning_with_render_muted() {
    use super::super::markdown::MarkdownRenderer;
    use super::super::style_tokens::{Indent, THINKING_BG};

    let content = "Let me think about this problem.\nFirst, I need to understand the requirements.\nThen I'll design the solution.";
    let md_lines = MarkdownRenderer::render_muted(content, THINKING_BG);

    let thinking_style = Style::default().fg(THINKING_BG);
    let first_prefix = vec![Span::styled("⟡ ", thinking_style)];
    let cont_prefix = vec![Span::styled(Indent::THINKING_CONT, thinking_style)];

    let result = wrap_spans_to_lines(md_lines, first_prefix, cont_prefix, 120);

    // Should produce lines (3 content lines)
    assert!(
        result.len() >= 3,
        "expected at least 3 lines, got {}",
        result.len()
    );

    // First line should have ⟡ prefix
    assert!(
        result[0].spans[0].content.as_ref().starts_with('⟡'),
        "first line should start with ⟡, got: {:?}",
        result[0].spans[0].content
    );

    // All continuation lines should have │ prefix
    for (i, line) in result.iter().enumerate().skip(1) {
        assert!(
            line.spans[0].content.as_ref().starts_with('│'),
            "line {i} should start with │, got: {:?}",
            line.spans[0].content
        );
    }

    // Content text should be preserved
    let all_text: String = result
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        all_text.contains("think about this problem"),
        "content lost: {all_text}"
    );
    assert!(
        all_text.contains("understand the requirements"),
        "content lost: {all_text}"
    );

    // Muted style should be preserved on content spans (not just prefix)
    for line in &result {
        for span in &line.spans {
            let text = span.content.as_ref();
            if !text.is_empty() && text != "⟡ " && text != Indent::THINKING_CONT {
                assert_eq!(
                    span.style.fg,
                    Some(THINKING_BG),
                    "span '{text}' lost muted fg color, style: {:?}",
                    span.style
                );
            }
        }
    }
}

#[test]
fn test_bullet_indent_stripped() {
    // Markdown renderer produces "  - " (2-space indent + dash + space).
    // The outer cont_prefix is "  " (2 chars). The structural prefix
    // should strip the redundant 2 leading spaces so the dash lands at col 2.
    // Bullets come after a header line so they use cont_prefix.
    let md_lines = vec![
        Line::from(vec![Span::raw("Header line")]),
        Line::from(vec![Span::raw("  - "), Span::raw("Bullet text here")]),
    ];
    let first = vec![Span::raw("⏺ ")];
    let cont = vec![Span::raw("  ")];

    let result = wrap_spans_to_lines(md_lines, first, cont, 80);
    assert_eq!(result.len(), 2);
    let text: String = result[1].spans.iter().map(|s| s.content.as_ref()).collect();
    // Should be "  - Bullet text here" (cont_prefix "  " + stripped "- " + content)
    assert_eq!(text, "  - Bullet text here");
}

#[test]
fn test_bullet_wrap_alignment() {
    // A long bullet line should wrap with continuation aligned at col 4
    // (2 for cont_prefix + 2 for "- ").
    let long_text = "word ".repeat(15).trim().to_string();
    let md_lines = vec![
        Line::from(vec![Span::raw("Header line")]),
        Line::from(vec![Span::raw("  - "), Span::raw(long_text)]),
    ];
    let first = vec![Span::raw("⏺ ")];
    let cont = vec![Span::raw("  ")];

    let result = wrap_spans_to_lines(md_lines, first, cont, 40);
    // Header + at least 2 bullet lines (first + wrap)
    assert!(
        result.len() >= 3,
        "should have wrapped, got {} lines",
        result.len()
    );

    // Second line (first bullet line): cont_prefix + "- " + content
    let bullet_text: String = result[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        bullet_text.starts_with("  - "),
        "bullet line should start with '  - ', got: {bullet_text}"
    );

    // Continuation lines of the bullet: 4 spaces padding (aligned with content after "- ")
    for i in 2..result.len() {
        let line_text: String = result[i].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            line_text.starts_with("    "),
            "continuation line {i} should start with 4 spaces, got: {:?}",
            line_text
        );
    }

    // All lines fit within max_width
    for line in &result {
        let w: usize = line.spans.iter().map(|s| span_width(s)).sum();
        assert!(w <= 40, "line width {w} exceeds 40");
    }
}

#[test]
fn test_nested_bullet_alignment() {
    // Nested bullet: "    - " (4-space indent). After stripping 2 (cont_prefix_w),
    // we get "  - " (2-space indent + dash). So nested dash at col 4, text at col 6.
    let long_text = "word ".repeat(15).trim().to_string();
    let md_lines = vec![
        Line::from(vec![Span::raw("Header line")]),
        Line::from(vec![Span::raw("    - "), Span::raw(long_text)]),
    ];
    let first = vec![Span::raw("⏺ ")];
    let cont = vec![Span::raw("  ")];

    let result = wrap_spans_to_lines(md_lines, first, cont, 40);
    assert!(
        result.len() >= 3,
        "should have wrapped, got {} lines",
        result.len()
    );

    // Second line: cont_prefix "  " + stripped "  - " = "    - " + content
    let bullet_text: String = result[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        bullet_text.starts_with("    - "),
        "first should start with '    - ', got: {bullet_text}"
    );

    // Continuation: 6 spaces (2 cont_prefix + 4 stripped prefix width "  - ")
    for i in 2..result.len() {
        let line_text: String = result[i].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            line_text.starts_with("      "),
            "nested continuation line {i} should start with 6 spaces, got: {:?}",
            line_text
        );
    }
}
