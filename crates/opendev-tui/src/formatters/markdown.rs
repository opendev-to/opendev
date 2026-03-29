//! Markdown rendering for terminal output.
//!
//! Converts markdown text to styled ratatui `Line`s with basic formatting:
//! headers, bold, italic, code blocks, and inline code.

use std::borrow::Cow;

use super::style_tokens;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// A color palette for markdown rendering.
/// The default uses the standard bright colors; `muted()` produces a
/// subdued palette suitable for thinking/reasoning display.
#[derive(Debug, Clone)]
pub struct MdPalette {
    pub heading: Color,
    pub heading_2: Color,
    pub heading_3: Color,
    pub code_fg: Color,
    pub code_bg: Color,
    pub bullet: Color,
    pub bold_fg: Color,
    pub link: Color,
    pub text: Color,
    /// Extra modifier applied to every span (e.g. `ITALIC` for thinking).
    pub base_modifier: Modifier,
}

impl Default for MdPalette {
    fn default() -> Self {
        Self {
            heading: style_tokens::HEADING_1,
            heading_2: style_tokens::HEADING_2,
            heading_3: style_tokens::HEADING_3,
            code_fg: style_tokens::CODE_FG,
            code_bg: style_tokens::CODE_BG,
            bullet: style_tokens::BULLET,
            bold_fg: style_tokens::BOLD_FG,
            link: style_tokens::BLUE_BRIGHT,
            text: style_tokens::PRIMARY,
            base_modifier: Modifier::empty(),
        }
    }
}

impl MdPalette {
    /// Build a muted palette for thinking/reasoning display.
    /// Uses the given `base` color for text and derives dimmed variants
    /// for structural elements.
    /// Build a muted palette for thinking/reasoning display.
    /// Uses the given `base` color for text and derives dimmed variants
    /// for structural elements.
    pub fn muted(base: Color) -> Self {
        // Derive slightly brighter heading from the base for contrast
        let heading = dim_color(style_tokens::HEADING_1, 0.50);
        let heading_2 = dim_color(style_tokens::HEADING_2, 0.50);
        let heading_3 = dim_color(style_tokens::HEADING_3, 0.50);
        let code_fg = dim_color(style_tokens::CODE_FG, 0.50);
        let bold_fg = dim_color(style_tokens::BOLD_FG, 0.55);
        let link = dim_color(style_tokens::BLUE_BRIGHT, 0.50);
        Self {
            heading,
            heading_2,
            heading_3,
            code_fg,
            code_bg: style_tokens::CODE_BG,
            bullet: base,
            bold_fg,
            link,
            text: base,
            base_modifier: Modifier::empty(),
        }
    }
}

/// Dim an RGB color by mixing it toward black. `factor` in 0.0..=1.0.
fn dim_color(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        ),
        other => other,
    }
}

/// Renders markdown text into styled terminal lines.
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    /// Render markdown text into a vector of styled lines using the default palette.
    ///
    /// Span content uses `Cow<'static, str>` where possible to reduce
    /// intermediate string allocations through the parsing pipeline.
    pub fn render(text: &str) -> Vec<Line<'static>> {
        Self::render_with_palette(text, &MdPalette::default())
    }

    /// Render markdown with a muted palette (for thinking/reasoning display).
    pub fn render_muted(text: &str, base_color: Color) -> Vec<Line<'static>> {
        Self::render_with_palette(text, &MdPalette::muted(base_color))
    }

    /// Render markdown text with a given color palette.
    pub fn render_with_palette(text: &str, palette: &MdPalette) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let mut in_code_block = false;
        let base_mod = palette.base_modifier;

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
                            Style::default()
                                .fg(style_tokens::GREY)
                                .add_modifier(base_mod),
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
                        .fg(palette.code_fg)
                        .bg(palette.code_bg)
                        .add_modifier(base_mod),
                )));
                continue;
            }

            // Horizontal rules (---, ***, ___)
            if is_horizontal_rule(raw_line) {
                let rule: Cow<'static, str> = Cow::Borrowed("────────────────────────────────");
                lines.push(Line::from(Span::styled(
                    rule,
                    Style::default()
                        .fg(style_tokens::GREY)
                        .add_modifier(base_mod),
                )));
                continue;
            }

            // Blockquotes (> text)
            if let Some(quote_content) = raw_line
                .strip_prefix("> ")
                .or_else(|| if raw_line == ">" { Some("") } else { None })
            {
                let mut spans = vec![Span::styled(
                    Cow::<'static, str>::Borrowed("  │ "),
                    Style::default()
                        .fg(style_tokens::GREY)
                        .add_modifier(base_mod),
                )];
                spans.extend(parse_inline_spans_with_palette(quote_content, palette));
                lines.push(Line::from(spans));
                continue;
            }

            // Headers — each level gets a distinct style for visual hierarchy
            if let Some(header) = raw_line.strip_prefix("### ") {
                // Blank line before header (if not first line)
                if !lines.is_empty() {
                    lines.push(Line::from(""));
                }
                let h: Cow<'static, str> = Cow::Owned(header.to_string());
                lines.push(Line::from(Span::styled(
                    h,
                    Style::default()
                        .fg(palette.heading_3)
                        .add_modifier(Modifier::BOLD | base_mod),
                )));
                // Blank line after header
                lines.push(Line::from(""));
            } else if let Some(header) = raw_line.strip_prefix("## ") {
                if !lines.is_empty() {
                    lines.push(Line::from(""));
                }
                let h: Cow<'static, str> = Cow::Owned(header.to_string());
                lines.push(Line::from(Span::styled(
                    h,
                    Style::default()
                        .fg(palette.heading_2)
                        .add_modifier(Modifier::BOLD | base_mod),
                )));
                lines.push(Line::from(""));
            } else if let Some(header) = raw_line.strip_prefix("# ") {
                if !lines.is_empty() {
                    lines.push(Line::from(""));
                }
                let h: Cow<'static, str> = Cow::Owned(header.to_string());
                lines.push(Line::from(Span::styled(
                    h,
                    Style::default()
                        .fg(palette.heading)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED | base_mod),
                )));
                lines.push(Line::from(""));
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
                    Style::default().fg(palette.bullet).add_modifier(base_mod),
                )];
                spans.extend(parse_inline_spans_with_palette(content, palette));
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
                    Style::default().fg(palette.bullet).add_modifier(base_mod),
                )];
                spans.extend(parse_inline_spans_with_palette(content, palette));
                lines.push(Line::from(spans));
            } else {
                // Regular text with inline formatting
                lines.push(render_inline_line_with_palette(raw_line, palette));
            }
        }

        lines
    }
}

/// Render inline formatting with a custom palette.
fn render_inline_line_with_palette(text: &str, palette: &MdPalette) -> Line<'static> {
    let spans = parse_inline_spans_with_palette(text, palette);
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
/// Delegates to [`parse_inline_spans_with_palette`] with the default palette.
#[cfg(test)]
fn parse_inline_spans(text: &str) -> Vec<Span<'static>> {
    parse_inline_spans_with_palette(text, &MdPalette::default())
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

/// Parse inline spans with a custom palette.
///
/// Handles `**bold**`, `*italic*`, `` `code` ``, and `[link](url)` markers by
/// scanning for the earliest marker and toggling bold/italic state. Backticks
/// and links inside bold/italic regions inherit the current modifier state.
fn parse_inline_spans_with_palette(text: &str, palette: &MdPalette) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let base_mod = palette.base_modifier;
    let mut pos = 0;
    let mut in_bold = false;
    let mut in_italic = false;
    // Track where the current bold/italic region opened, for fallback
    let mut bold_open_pos: Option<usize> = None;
    let mut italic_open_pos: Option<usize> = None;
    let bytes = text.as_bytes();

    // Helper: compute the style for plain text at the current bold/italic state
    let text_style = |bold: bool, italic: bool| -> Style {
        let mut style = Style::default().add_modifier(base_mod);
        if bold {
            style = style.fg(palette.bold_fg).add_modifier(Modifier::BOLD);
        } else {
            style = style.fg(palette.text);
        }
        if italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    };

    let mut plain_start = 0; // start of current plain-text accumulation

    while pos < text.len() {
        // Find the earliest marker from current position
        let next_star = find_byte(bytes, b'*', pos);
        let next_backtick = find_byte(bytes, b'`', pos);
        let next_link =
            find_markdown_link(&text[pos..]).map(|(s, t, u, e)| (s + pos, t, u, e + pos));

        // Pick the earliest marker
        let candidates: [Option<usize>; 3] = [
            next_star,
            next_backtick,
            next_link.as_ref().map(|(s, _, _, _)| *s),
        ];
        let earliest = candidates.into_iter().flatten().min();

        let Some(marker_pos) = earliest else {
            // No more markers — flush remaining text
            break;
        };

        // Which marker is at this position? Handle star runs by counting consecutive stars.
        if next_star == Some(marker_pos) {
            // Count consecutive stars
            let star_count = bytes[marker_pos..]
                .iter()
                .take_while(|&&b| b == b'*')
                .count();

            // Flush plain text before the marker
            if marker_pos > plain_start {
                let chunk: Cow<'static, str> =
                    Cow::Owned(text[plain_start..marker_pos].to_string());
                spans.push(Span::styled(chunk, text_style(in_bold, in_italic)));
            }

            // Consume stars: ** = bold, * = italic
            // 1=italic, 2=bold, 3=bold+italic, 4=bold+bold, 5=bold+bold+italic
            let mut remaining_stars = star_count;
            while remaining_stars >= 2 {
                in_bold = !in_bold;
                remaining_stars -= 2;
            }
            if remaining_stars == 1 {
                in_italic = !in_italic;
            }
            // Update tracking positions
            if in_bold {
                if bold_open_pos.is_none() {
                    bold_open_pos = Some(marker_pos);
                }
            } else {
                bold_open_pos = None;
            }
            if in_italic {
                if italic_open_pos.is_none() {
                    italic_open_pos = Some(marker_pos);
                }
            } else {
                italic_open_pos = None;
            }
            pos = marker_pos + star_count;
            plain_start = pos;
        } else if next_backtick == Some(marker_pos) {
            // Flush plain text before the backtick
            if marker_pos > plain_start {
                let chunk: Cow<'static, str> =
                    Cow::Owned(text[plain_start..marker_pos].to_string());
                spans.push(Span::styled(chunk, text_style(in_bold, in_italic)));
            }
            // Count consecutive backticks to support multi-backtick code spans (`` `code` ``)
            let bt_count = bytes[marker_pos..]
                .iter()
                .take_while(|&&b| b == b'`')
                .count();
            let after = marker_pos + bt_count;
            let closing_pattern = &text[marker_pos..marker_pos + bt_count]; // e.g. "``"
            if let Some(close_rel) = text[after..].find(closing_pattern) {
                let close = after + close_rel;
                // Strip one leading/trailing space for multi-backtick (CommonMark rule)
                let mut code_start = after;
                let mut code_end = close;
                if bt_count > 1 && code_end > code_start {
                    if bytes.get(code_start) == Some(&b' ') {
                        code_start += 1;
                    }
                    if code_end > code_start && bytes.get(code_end - 1) == Some(&b' ') {
                        code_end -= 1;
                    }
                }
                let code: Cow<'static, str> = Cow::Owned(text[code_start..code_end].to_string());
                let mut style = Style::default().fg(palette.code_fg).add_modifier(base_mod);
                if in_bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if in_italic {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                spans.push(Span::styled(code, style));
                pos = close + bt_count;
                plain_start = pos;
            } else {
                // No closing backtick(s) — treat as plain text, continue
                pos = marker_pos + 1;
            }
        } else if let Some((link_start, link_text, _url, link_end)) = next_link {
            if link_start == marker_pos {
                // Flush plain text before the link
                if link_start > plain_start {
                    let chunk: Cow<'static, str> =
                        Cow::Owned(text[plain_start..link_start].to_string());
                    spans.push(Span::styled(chunk, text_style(in_bold, in_italic)));
                }
                let display: Cow<'static, str> = Cow::Owned(link_text.to_string());
                let mut style = Style::default().fg(palette.link).add_modifier(base_mod);
                if in_bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if in_italic {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                spans.push(Span::styled(display, style));
                pos = link_end;
                plain_start = pos;
            } else {
                // Link is not the earliest — skip past marker_pos
                pos = marker_pos + 1;
            }
        } else {
            pos = marker_pos + 1;
        }
    }

    // Flush remaining plain text
    // If bold/italic is still open, we have unmatched markers — re-emit from the opening position
    if in_bold {
        // Unmatched ** — re-emit the opening marker as literal text
        if plain_start < text.len() {
            let chunk: Cow<'static, str> = Cow::Owned(format!("**{}", &text[plain_start..]));
            spans.push(Span::styled(chunk, text_style(false, in_italic)));
        }
        let _ = bold_open_pos;
    } else if plain_start < text.len() {
        let chunk: Cow<'static, str> = Cow::Owned(text[plain_start..].to_string());
        spans.push(Span::styled(chunk, text_style(in_bold, in_italic)));
    }

    // Unmatched italic: the remaining text was already flushed above
    // with the italic style. This is acceptable — single * at end of line is rare.
    let _ = (in_italic, italic_open_pos);

    if spans.is_empty() {
        spans.push(Span::styled(
            Cow::Owned(String::new()),
            Style::default().add_modifier(base_mod),
        ));
    }

    spans
}

/// Find a single byte in a byte slice starting from `from`.
fn find_byte(haystack: &[u8], needle: u8, from: usize) -> Option<usize> {
    haystack[from..]
        .iter()
        .position(|&b| b == needle)
        .map(|p| p + from)
}

/// Check if a line is a horizontal rule (---, ***, ___).
fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.chars().next().unwrap();
    matches!(first, '-' | '*' | '_')
        && trimmed.chars().all(|c| c == first || c == ' ')
        && trimmed.chars().filter(|&c| c == first).count() >= 3
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
