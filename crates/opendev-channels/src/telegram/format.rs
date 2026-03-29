//! Markdown to Telegram HTML conversion and HTML-aware message splitting.
//!
//! Telegram supports a subset of HTML: `<b>`, `<i>`, `<s>`, `<u>`, `<code>`,
//! `<pre>`, `<blockquote>`, `<a href="...">`. This module converts standard
//! markdown (as output by LLMs) into that subset, then splits long messages
//! while preserving valid tag nesting across chunk boundaries.

/// Maximum message length for Telegram messages.
const TELEGRAM_MAX_LEN: usize = 4096;

/// Convert markdown text to Telegram-compatible HTML.
///
/// Handles: bold, italic, strikethrough, inline code, fenced code blocks,
/// blockquotes, links, and headings. Characters that conflict with HTML
/// (`<`, `>`, `&`) are escaped first so raw text passes through safely.
pub fn markdown_to_telegram_html(md: &str) -> String {
    let mut out = String::with_capacity(md.len() + md.len() / 4);
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;
    let mut in_blockquote = false;

    while i < lines.len() {
        let line = lines[i];

        // ── Fenced code blocks ──
        if line.trim_start().starts_with("```") {
            let lang = line.trim_start().trim_start_matches('`').trim();
            if !lang.is_empty() {
                out.push_str(&format!(
                    "<pre><code class=\"language-{}\">",
                    escape_html(lang)
                ));
            } else {
                out.push_str("<pre><code>");
            }
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                out.push_str(&escape_html(lines[i]));
                out.push('\n');
                i += 1;
            }
            if out.ends_with('\n') {
                out.pop();
            }
            out.push_str("</code></pre>\n");
            i += 1;
            continue;
        }

        // ── Blockquotes ──
        if let Some(rest) = line.strip_prefix("> ").or_else(|| line.strip_prefix(">")) {
            if !in_blockquote {
                out.push_str("<blockquote>");
                in_blockquote = true;
            } else {
                out.push('\n');
            }
            out.push_str(&convert_inline(&escape_html(rest)));
            i += 1;
            continue;
        } else if in_blockquote {
            out.push_str("</blockquote>\n");
            in_blockquote = false;
        }

        // ── Headings → bold ──
        if let Some(heading) = strip_heading(line) {
            out.push_str(&format!(
                "<b>{}</b>\n",
                convert_inline(&escape_html(heading))
            ));
            i += 1;
            continue;
        }

        // ── Regular line ──
        out.push_str(&convert_inline(&escape_html(line)));
        out.push('\n');
        i += 1;
    }

    if in_blockquote {
        out.push_str("</blockquote>\n");
    }

    while out.ends_with('\n') {
        out.pop();
    }

    out
}

/// Escape HTML special characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Strip markdown heading prefix (# to ######), return the heading text.
fn strip_heading(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    for prefix in &["###### ", "##### ", "#### ", "### ", "## ", "# "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some(rest.trim_start());
        }
    }
    None
}

/// Convert inline markdown to Telegram HTML (operates on already-escaped text).
fn convert_inline(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Inline code
        if chars[i] == '`'
            && let Some(end) = find_closing(&chars[i + 1..], '`').map(|p| p + i + 1)
        {
            let code_text: String = chars[i + 1..end].iter().collect();
            result.push_str(&format!("<code>{code_text}</code>"));
            i = end + 1;
            continue;
        }

        // Links: [text](url)
        if chars[i] == '['
            && let Some((link_text, url, end)) = parse_link(&chars, i)
        {
            result.push_str(&format!("<a href=\"{url}\">{link_text}</a>"));
            i = end;
            continue;
        }

        // Bold: **text**
        if i + 1 < len
            && chars[i] == '*'
            && chars[i + 1] == '*'
            && let Some(end) = find_double_marker(&chars, i + 2, '*')
        {
            let inner: String = chars[i + 2..end].iter().collect();
            result.push_str(&format!("<b>{}</b>", convert_inline(&inner)));
            i = end + 2;
            continue;
        }

        // Strikethrough: ~~text~~
        if i + 1 < len
            && chars[i] == '~'
            && chars[i + 1] == '~'
            && let Some(end) = find_double_marker(&chars, i + 2, '~')
        {
            let inner: String = chars[i + 2..end].iter().collect();
            result.push_str(&format!("<s>{}</s>", convert_inline(&inner)));
            i = end + 2;
            continue;
        }

        // Italic: *text*
        if chars[i] == '*'
            && i + 1 < len
            && chars[i + 1] != ' '
            && let Some(end) = find_single_marker(&chars, i + 1, '*')
        {
            let inner: String = chars[i + 1..end].iter().collect();
            result.push_str(&format!("<i>{}</i>", convert_inline(&inner)));
            i = end + 1;
            continue;
        }

        // Italic: _text_
        if chars[i] == '_'
            && i + 1 < len
            && chars[i + 1] != ' '
            && let Some(end) = find_single_marker(&chars, i + 1, '_')
        {
            let inner: String = chars[i + 1..end].iter().collect();
            result.push_str(&format!("<i>{}</i>", convert_inline(&inner)));
            i = end + 1;
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

fn find_closing(chars: &[char], marker: char) -> Option<usize> {
    chars.iter().position(|&c| c == marker)
}

fn find_single_marker(chars: &[char], start: usize, marker: char) -> Option<usize> {
    (start..chars.len()).find(|&i| chars[i] == marker && (i == 0 || chars[i - 1] != ' '))
}

fn find_double_marker(chars: &[char], start: usize, marker: char) -> Option<usize> {
    (start..chars.len().saturating_sub(1)).find(|&i| chars[i] == marker && chars[i + 1] == marker)
}

fn parse_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let mut i = start + 1;
    while i < chars.len() && chars[i] != ']' {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    let text: String = chars[start + 1..i].iter().collect();
    i += 1;

    if i >= chars.len() || chars[i] != '(' {
        return None;
    }
    i += 1;

    let url_start = i;
    while i < chars.len() && chars[i] != ')' {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    let url: String = chars[url_start..i].iter().collect();
    i += 1;

    Some((text, url, i))
}

/// Split Telegram HTML into chunks that respect tag boundaries.
///
/// Closes open tags at the end of each chunk and reopens them at the start
/// of the next chunk, ensuring each chunk is valid HTML.
pub fn split_telegram_html(html: &str, max_len: usize) -> Vec<String> {
    let max_len = max_len.min(TELEGRAM_MAX_LEN);

    if html.len() <= max_len {
        return vec![html.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = html.to_string();

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining);
            break;
        }

        let split_at = find_safe_html_split(&remaining, max_len);
        let chunk = &remaining[..split_at];
        let rest = &remaining[split_at..];

        let open_tags = find_open_tags(chunk);

        // Close open tags at chunk boundary
        let mut closed_chunk = chunk.to_string();
        for tag in open_tags.iter().rev() {
            closed_chunk.push_str(&format!("</{tag}>"));
        }
        chunks.push(closed_chunk);

        // Reopen tags at start of next chunk
        let mut next = String::new();
        for tag in &open_tags {
            next.push_str(&format!("<{tag}>"));
        }
        next.push_str(rest.trim_start_matches('\n'));
        remaining = next;
    }

    chunks
}

/// Find a safe position to split HTML, avoiding splitting inside tags.
fn find_safe_html_split(html: &str, max_len: usize) -> usize {
    let target = max_len.min(html.len());
    let bytes = html.as_bytes();

    // Search backward for a newline not inside a tag
    for pos in (0..target).rev() {
        if bytes[pos] == b'\n' && !is_inside_tag(html, pos) {
            return pos + 1;
        }
    }

    // Fall back to space
    for pos in (target / 2..target).rev() {
        if bytes[pos] == b' ' && !is_inside_tag(html, pos) {
            return pos + 1;
        }
    }

    // Hard split at target
    target.max(1)
}

/// Check if a position is inside an HTML tag.
fn is_inside_tag(html: &str, pos: usize) -> bool {
    let before = &html[..pos];
    let last_open = before.rfind('<');
    let last_close = before.rfind('>');

    match (last_open, last_close) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Find tags that are opened but not closed in the given HTML fragment.
fn find_open_tags(html: &str) -> Vec<String> {
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    let bytes = html.as_bytes();

    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(tag_end) = html[i..].find('>')
        {
            let tag_content = &html[i + 1..i + tag_end];

            if let Some(tag_name) = tag_content.strip_prefix('/') {
                let name = tag_name.trim().to_lowercase();
                if let Some(pos) = stack.iter().rposition(|t| *t == name) {
                    stack.remove(pos);
                }
            } else {
                let name = tag_content
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                if !name.is_empty() && !is_self_closing(&name) {
                    stack.push(name);
                }
            }

            i += tag_end + 1;
            continue;
        }
        i += 1;
    }

    stack
}

fn is_self_closing(tag: &str) -> bool {
    matches!(tag, "br" | "hr" | "img")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bold() {
        assert_eq!(markdown_to_telegram_html("**hello**"), "<b>hello</b>");
    }

    #[test]
    fn test_italic_asterisk() {
        assert_eq!(markdown_to_telegram_html("*hello*"), "<i>hello</i>");
    }

    #[test]
    fn test_italic_underscore() {
        assert_eq!(markdown_to_telegram_html("_hello_"), "<i>hello</i>");
    }

    #[test]
    fn test_inline_code() {
        assert_eq!(markdown_to_telegram_html("`code`"), "<code>code</code>");
    }

    #[test]
    fn test_strikethrough() {
        assert_eq!(markdown_to_telegram_html("~~deleted~~"), "<s>deleted</s>");
    }

    #[test]
    fn test_fenced_code_block() {
        let md = "```rust\nfn main() {}\n```";
        let html = markdown_to_telegram_html(md);
        assert_eq!(
            html,
            "<pre><code class=\"language-rust\">fn main() {}</code></pre>"
        );
    }

    #[test]
    fn test_fenced_code_block_no_lang() {
        let md = "```\nsome code\n```";
        let html = markdown_to_telegram_html(md);
        assert_eq!(html, "<pre><code>some code</code></pre>");
    }

    #[test]
    fn test_heading() {
        assert_eq!(markdown_to_telegram_html("## Title"), "<b>Title</b>");
    }

    #[test]
    fn test_blockquote() {
        assert_eq!(
            markdown_to_telegram_html("> quoted text"),
            "<blockquote>quoted text</blockquote>"
        );
    }

    #[test]
    fn test_link() {
        assert_eq!(
            markdown_to_telegram_html("[click](https://example.com)"),
            "<a href=\"https://example.com\">click</a>"
        );
    }

    #[test]
    fn test_html_escaping() {
        assert_eq!(
            markdown_to_telegram_html("a < b & c > d"),
            "a &lt; b &amp; c &gt; d"
        );
    }

    #[test]
    fn test_mixed_formatting() {
        assert_eq!(
            markdown_to_telegram_html("**bold** and *italic* and `code`"),
            "<b>bold</b> and <i>italic</i> and <code>code</code>"
        );
    }

    #[test]
    fn test_split_short() {
        let chunks = split_telegram_html("hello", 4096);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn test_find_open_tags() {
        let tags = find_open_tags("<b>hello <i>world");
        assert_eq!(tags, vec!["b", "i"]);
    }

    #[test]
    fn test_find_open_tags_closed() {
        let tags = find_open_tags("<b>hello</b>");
        assert!(tags.is_empty());
    }

    #[test]
    fn test_code_block_preserves_content() {
        let md = "```\n<script>alert('xss')</script>\n```";
        let html = markdown_to_telegram_html(md);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
