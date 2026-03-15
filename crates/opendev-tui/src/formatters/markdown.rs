//! Markdown rendering for terminal output.
//!
//! Converts markdown text to styled ratatui `Line`s with basic formatting:
//! headers, bold, italic, code blocks, and inline code.

use std::borrow::Cow;

use super::style_tokens;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

/// Renders markdown text into styled terminal lines.
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    /// Render markdown text into a vector of styled lines.
    ///
    /// Span content uses `Cow<'static, str>` where possible to reduce
    /// intermediate string allocations through the parsing pipeline.
    pub fn render(text: &str) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let mut in_code_block = false;

        for raw_line in text.lines() {
            if raw_line.starts_with("```") {
                in_code_block = !in_code_block;
                if in_code_block {
                    // Code block start — show language hint if present
                    let lang = raw_line.trim_start_matches('`').trim();
                    if !lang.is_empty() {
                        let hint: Cow<'static, str> = Cow::Owned(format!("--- {lang} ---"));
                        lines.push(Line::from(Span::styled(
                            hint,
                            Style::default().fg(style_tokens::GREY),
                        )));
                    }
                }
                continue;
            }

            if in_code_block {
                let code: Cow<'static, str> = Cow::Owned(raw_line.to_string());
                lines.push(Line::from(Span::styled(
                    code,
                    Style::default()
                        .fg(style_tokens::CODE_FG)
                        .bg(style_tokens::CODE_BG),
                )));
                continue;
            }

            // Headers
            if let Some(header) = raw_line.strip_prefix("### ") {
                let h: Cow<'static, str> = Cow::Owned(header.to_string());
                lines.push(Line::from(Span::styled(
                    h,
                    Style::default()
                        .fg(style_tokens::HEADING_3)
                        .add_modifier(Modifier::BOLD),
                )));
            } else if let Some(header) = raw_line.strip_prefix("## ") {
                let h: Cow<'static, str> = Cow::Owned(header.to_string());
                lines.push(Line::from(Span::styled(
                    h,
                    Style::default()
                        .fg(style_tokens::HEADING_2)
                        .add_modifier(Modifier::BOLD),
                )));
            } else if let Some(header) = raw_line.strip_prefix("# ") {
                let h: Cow<'static, str> = Cow::Owned(header.to_string());
                lines.push(Line::from(Span::styled(
                    h,
                    Style::default()
                        .fg(style_tokens::HEADING_1)
                        .add_modifier(Modifier::BOLD),
                )));
            } else if is_bullet_line(raw_line) {
                // Bullet list (supports nesting)
                let trimmed = raw_line.trim_start();
                let indent_len = raw_line.len() - trimmed.len();
                let indent_level = indent_len / 2;
                let content = &trimmed[2..];
                let prefix: Cow<'static, str> = if indent_level == 0 {
                    Cow::Borrowed("  - ")
                } else {
                    Cow::Owned(format!("{}  - ", "  ".repeat(indent_level)))
                };
                let mut spans = vec![Span::styled(
                    prefix,
                    Style::default().fg(style_tokens::BULLET),
                )];
                spans.extend(parse_inline_spans(content));
                lines.push(Line::from(spans));
            } else if is_ordered_list_line(raw_line) {
                // Ordered list
                let trimmed = raw_line.trim_start();
                let indent_len = raw_line.len() - trimmed.len();
                let indent_level = indent_len / 2;
                let dot_pos = trimmed.find(". ").unwrap();
                let number = &trimmed[..dot_pos];
                let content = &trimmed[dot_pos + 2..];
                let prefix: Cow<'static, str> =
                    Cow::Owned(format!("{}  {}. ", "  ".repeat(indent_level), number));
                let mut spans = vec![Span::styled(
                    prefix,
                    Style::default().fg(style_tokens::BULLET),
                )];
                spans.extend(parse_inline_spans(content));
                lines.push(Line::from(spans));
            } else {
                // Regular text with inline formatting
                lines.push(render_inline_line(raw_line));
            }
        }

        lines
    }
}

/// Render inline formatting (bold, italic, code) in a single line.
fn render_inline_line(text: &str) -> Line<'static> {
    // Simple approach: split by backtick pairs for inline code
    let spans = parse_inline_spans(text);
    Line::from(spans)
}

/// Check if a line is a bullet list item (possibly indented).
fn is_bullet_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ")
}

/// Check if a line is an ordered list item (possibly indented).
fn is_ordered_list_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if let Some(dot_pos) = trimmed.find(". ") {
        dot_pos > 0 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

/// Parse inline spans handling markdown links, backtick code, and bold markers.
///
/// Uses `Cow<'static, str>` internally: substrings that require no
/// transformation are converted to owned `String`s only once (at the Span
/// boundary), which is unavoidable for `Span<'static>`. The `Cow` usage
/// makes the intent clear and avoids redundant intermediate allocations
/// when the same string passes through multiple parsing layers.
fn parse_inline_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find the next interesting marker: backtick or markdown link
        let next_backtick = remaining.find('`');
        let next_link = find_markdown_link(remaining);

        // Determine which marker comes first
        let use_link = match (next_backtick, &next_link) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(bt), Some((ls, _, _, _))) => *ls < bt,
        };

        if use_link {
            let (link_start, link_text, _url, link_end) = next_link.unwrap();
            if link_start > 0 {
                spans.extend(parse_bold_spans(&remaining[..link_start]));
            }
            let display: Cow<'static, str> = Cow::Owned(link_text.to_string());
            spans.push(Span::styled(
                display,
                Style::default()
                    .fg(style_tokens::BLUE_BRIGHT)
                    .add_modifier(Modifier::UNDERLINED),
            ));
            remaining = &remaining[link_end..];
        } else if let Some(code_start) = next_backtick {
            if code_start > 0 {
                spans.extend(parse_bold_spans(&remaining[..code_start]));
            }
            let after_start = &remaining[code_start + 1..];
            if let Some(code_end) = after_start.find('`') {
                let code: Cow<'static, str> = Cow::Owned(after_start[..code_end].to_string());
                spans.push(Span::styled(
                    code,
                    Style::default()
                        .fg(style_tokens::CODE_FG)
                        .bg(style_tokens::CODE_BG),
                ));
                remaining = &after_start[code_end + 1..];
            } else {
                spans.extend(parse_bold_spans(remaining));
                break;
            }
        } else {
            spans.extend(parse_bold_spans(remaining));
            break;
        }
    }

    if spans.is_empty() {
        spans.push(Span::raw(Cow::Owned(String::new())));
    }

    spans
}

/// Find a markdown link `[text](url)` in the given text.
/// Returns `(start, link_text, url, end)` where end is the byte offset past the closing `)`.
fn find_markdown_link(text: &str) -> Option<(usize, &str, &str, usize)> {
    let open_bracket = text.find('[')?;
    let after_bracket = &text[open_bracket + 1..];
    let close_bracket = after_bracket.find(']')?;
    let link_text = &after_bracket[..close_bracket];

    // The `](` must immediately follow the `]`
    let after_close = &after_bracket[close_bracket + 1..];
    if !after_close.starts_with('(') {
        return None;
    }
    let after_paren = &after_close[1..];
    let close_paren = after_paren.find(')')?;
    let url = &after_paren[..close_paren];

    // Total end offset: open_bracket + 1 + close_bracket + 1 + 1 + close_paren + 1
    let end = open_bracket + 1 + close_bracket + 1 + 1 + close_paren + 1;
    Some((open_bracket, link_text, url, end))
}

/// Parse bold markers (**text**) within a segment of text.
///
/// Uses `Cow<'static, str>` for span content to make allocation points explicit.
fn parse_bold_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if let Some(bold_start) = remaining.find("**") {
            if bold_start > 0 {
                let plain: Cow<'static, str> = Cow::Owned(remaining[..bold_start].to_string());
                spans.push(Span::raw(plain));
            }
            let after_start = &remaining[bold_start + 2..];
            if let Some(bold_end) = after_start.find("**") {
                let bold_text: Cow<'static, str> = Cow::Owned(after_start[..bold_end].to_string());
                spans.push(Span::styled(
                    bold_text,
                    Style::default()
                        .fg(style_tokens::BOLD_FG)
                        .add_modifier(Modifier::BOLD),
                ));
                remaining = &after_start[bold_end + 2..];
            } else {
                let rest: Cow<'static, str> = Cow::Owned(remaining.to_string());
                spans.push(Span::raw(rest));
                break;
            }
        } else {
            let rest: Cow<'static, str> = Cow::Owned(remaining.to_string());
            spans.push(Span::raw(rest));
            break;
        }
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let lines = MarkdownRenderer::render("Hello world");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_headers() {
        let lines = MarkdownRenderer::render("# Title\n## Subtitle\n### Section");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_code_block() {
        let md = "```rust\nfn main() {}\n```";
        let lines = MarkdownRenderer::render(md);
        // lang hint + code line = 2
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_bullet_list() {
        let md = "- item one\n- item two";
        let lines = MarkdownRenderer::render(md);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_nested_bullets() {
        let md = "- top\n  - nested\n    - deep";
        let lines = MarkdownRenderer::render(md);
        assert_eq!(lines.len(), 3);
        // Check prefixes
        let first: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            first.starts_with("  - "),
            "top-level should start with '  - '"
        );
        let second: String = lines[1]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            second.starts_with("    - "),
            "nested should start with '    - '"
        );
        let third: String = lines[2]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            third.starts_with("      - "),
            "deep nested should start with '      - '"
        );
    }

    #[test]
    fn test_ordered_list() {
        let md = "1. first\n2. second\n3. third";
        let lines = MarkdownRenderer::render(md);
        assert_eq!(lines.len(), 3);
        let first: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(first.contains("1. "));
        let second: String = lines[1]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(second.contains("2. "));
    }

    #[test]
    fn test_bullet_with_inline_formatting() {
        let md = "- **bold** and `code`";
        let lines = MarkdownRenderer::render(md);
        assert_eq!(lines.len(), 1);
        // Should have more than 2 spans (prefix + inline formatted content)
        assert!(
            lines[0].spans.len() > 2,
            "bullet content should preserve inline formatting"
        );
    }

    #[test]
    fn test_inline_code() {
        let spans = parse_inline_spans("use `tokio` for async");
        assert!(spans.len() >= 3);
    }

    #[test]
    fn test_bold_text() {
        let spans = parse_bold_spans("this is **bold** text");
        assert!(spans.len() >= 3);
    }

    #[test]
    fn test_markdown_link() {
        let spans = parse_inline_spans("visit [example](http://example.com) now");
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "visit example now");
        // The link span should be underlined
        let link_span = &spans[1];
        assert_eq!(link_span.content.as_ref(), "example");
        assert!(link_span.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn test_markdown_link_url_as_text() {
        // Common pattern: [http://url](http://url)
        let spans =
            parse_inline_spans("running at [http://localhost:5173/](http://localhost:5173/).");
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "running at http://localhost:5173/.");
    }
}
