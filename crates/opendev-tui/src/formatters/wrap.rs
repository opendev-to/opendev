//! Manual word-wrapping utility that preserves per-line prefix spans.
//!
//! Unlike ratatui's `Wrap { trim: false }`, this pre-wraps lines so each
//! visual line already contains the correct indentation prefix. This makes
//! ratatui's wrapping a no-op and gives us full control over continuation
//! indentation.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use super::style_tokens::CODE_BG;

/// Returns true if any span in `line` has a background color matching `CODE_BG`,
/// indicating this line is inside a code block and should not be word-wrapped.
fn is_code_line(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .any(|s| s.style.bg.is_some_and(|bg| bg == CODE_BG))
}

/// Compute the display width of a span's content using unicode widths.
fn span_width(s: &Span<'_>) -> usize {
    s.content
        .chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum()
}

/// Compute the total display width of a slice of spans.
fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| span_width(s)).sum()
}

/// Pre-wrap a sequence of markdown-rendered lines, prepending the appropriate
/// prefix to each output line.
///
/// - `md_lines`: the lines produced by `MarkdownRenderer::render()` (or `render_muted()`)
/// - `first_prefix`: spans to prepend to the very first non-empty content line
/// - `cont_prefix`: spans to prepend to all other lines (continuations + blank)
/// - `max_width`: the maximum display width (typically `terminal_width - 1`)
///
/// Lines whose spans contain a `CODE_BG` background are passed through without
/// wrapping — code blocks should be truncated, not reflowed.
pub fn wrap_spans_to_lines<'a>(
    md_lines: Vec<Line<'a>>,
    first_prefix: Vec<Span<'a>>,
    cont_prefix: Vec<Span<'a>>,
    max_width: usize,
) -> Vec<Line<'a>> {
    if max_width == 0 {
        return Vec::new();
    }

    let first_prefix_w = spans_width(&first_prefix);
    let cont_prefix_w = spans_width(&cont_prefix);
    let mut output: Vec<Line<'a>> = Vec::new();
    let mut leading_consumed = false;

    for md_line in md_lines {
        // Check if this line has visible content
        let line_text: String = md_line.spans.iter().map(|s| s.content.as_ref()).collect();
        let has_content = !line_text.trim().is_empty();

        // Determine which prefix to use
        let (prefix, prefix_w) = if !leading_consumed && has_content {
            leading_consumed = true;
            (first_prefix.clone(), first_prefix_w)
        } else {
            (cont_prefix.clone(), cont_prefix_w)
        };

        // Code lines: pass through without wrapping
        if is_code_line(&md_line) {
            let mut spans = prefix;
            spans.extend(md_line.spans);
            output.push(Line::from(spans));
            continue;
        }

        // Empty/blank lines: just prefix
        if !has_content {
            output.push(Line::from(prefix));
            continue;
        }

        // Available width for content after prefix
        let avail = max_width.saturating_sub(prefix_w);
        if avail == 0 {
            let mut spans = prefix;
            spans.extend(md_line.spans);
            output.push(Line::from(spans));
            continue;
        }

        // Wrap the content spans into chunks that fit within `avail`
        let wrapped = wrap_spans(md_line.spans, avail);

        for (i, chunk) in wrapped.into_iter().enumerate() {
            let pfx = if i == 0 {
                prefix.clone()
            } else {
                cont_prefix.clone()
            };
            let mut spans = pfx;
            spans.extend(chunk);
            output.push(Line::from(spans));
        }
    }

    output
}

/// Word-wrap a sequence of styled spans to fit within `max_width` display columns.
///
/// Returns a `Vec<Vec<Span>>` where each inner vec represents one visual line.
/// Breaks at word boundaries (spaces) when possible; falls back to mid-word
/// breaks when a single word exceeds `max_width`.
fn wrap_spans(spans: Vec<Span<'_>>, max_width: usize) -> Vec<Vec<Span<'_>>> {
    if max_width == 0 {
        return vec![spans];
    }

    // Flatten spans into segments: (text, style, is_space)
    // We work character-by-character but try to keep spans intact when possible.
    let mut result: Vec<Vec<Span>> = Vec::new();
    let mut current_line: Vec<Span> = Vec::new();
    let mut line_width: usize = 0;

    // Track the last word-boundary position for backtracking
    // We'll use a simpler approach: accumulate a "current word" buffer
    // and flush words to lines.

    // First, split all spans into word-level tokens preserving styles
    let tokens = tokenize_spans(&spans);

    for token in tokens {
        let token_w = token
            .chars()
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum::<usize>();

        if line_width + token_w <= max_width {
            // Token fits on current line
            if let Some(last) = current_line.last_mut()
                && last.style == find_style_for_pos(&spans, &token)
            {
                // Extend existing span with same style
                let mut s = last.content.to_string();
                s.push_str(&token);
                *last = Span::styled(s, last.style);
            } else {
                let style = find_style_for_pos(&spans, &token);
                current_line.push(Span::styled(token, style));
            }
            line_width += token_w;
        } else if token.trim().is_empty() {
            // It's whitespace that would overflow — start a new line
            // (don't include trailing space on current line)
            if !current_line.is_empty() {
                result.push(std::mem::take(&mut current_line));
            }
            line_width = 0;
        } else if token_w > max_width {
            // Word is wider than max_width — we need to split it
            // First flush current line if non-empty
            if !current_line.is_empty() {
                result.push(std::mem::take(&mut current_line));
                line_width = 0;
            }
            // Split the oversized word character by character
            let style = find_style_for_pos(&spans, &token);
            let mut chunk = String::new();
            let mut chunk_w = 0;
            for c in token.chars() {
                let cw = UnicodeWidthChar::width(c).unwrap_or(0);
                if chunk_w + cw > max_width && !chunk.is_empty() {
                    current_line.push(Span::styled(std::mem::take(&mut chunk), style));
                    result.push(std::mem::take(&mut current_line));
                    chunk_w = 0;
                }
                chunk.push(c);
                chunk_w += cw;
            }
            if !chunk.is_empty() {
                current_line.push(Span::styled(chunk, style));
                line_width = chunk_w;
            }
        } else {
            // Word doesn't fit — start a new line
            if !current_line.is_empty() {
                result.push(std::mem::take(&mut current_line));
            }
            let style = find_style_for_pos(&spans, &token);
            current_line.push(Span::styled(token, style));
            line_width = token_w;
        }
    }

    // Flush remaining
    if !current_line.is_empty() {
        result.push(current_line);
    }

    if result.is_empty() {
        result.push(Vec::new());
    }

    result
}

/// Find the style that applies to a given token text by scanning the original spans.
/// This is a heuristic: we find the first span that contains the token's first character.
fn find_style_for_pos<'a>(spans: &[Span<'a>], token: &str) -> Style {
    if token.is_empty() {
        return Style::default();
    }
    // Simple approach: find first span whose content contains this token
    // For correctness, we'd track byte positions, but for word-level tokens
    // this works well enough since tokens come from splitting span content.
    let first_char = token.chars().next().unwrap();
    let token_first_bytes = &token[..first_char.len_utf8()];

    let mut pos = 0usize;
    for span in spans {
        let span_str = span.content.as_ref();
        if let Some(idx) = span_str.find(token_first_bytes) {
            let _ = idx;
            return span.style;
        }
        pos += span_str.len();
    }
    let _ = pos;
    spans.last().map(|s| s.style).unwrap_or_default()
}

/// Tokenize spans into words and whitespace, preserving the original text exactly.
fn tokenize_spans(spans: &[Span<'_>]) -> Vec<String> {
    let full_text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_space = false;

    for c in full_text.chars() {
        let is_space = c == ' ';
        if is_space != in_space && !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
        current.push(c);
        in_space = is_space;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
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
}
