//! HTML-to-markdown conversion utilities.
//!
//! Regex-based extraction rather than a full DOM parser. Handles the most
//! common HTML patterns: headings, paragraphs, links, lists, code blocks,
//! emphasis, and removes scripts/styles/navigation.

use regex::Regex;

/// Convert HTML content to clean markdown for LLM-friendly output.
pub(super) fn html_to_markdown(html: &str) -> String {
    let mut text = html.to_string();

    // Remove script, style, nav, footer, header tags and their content
    for tag in &[
        "script", "style", "nav", "footer", "header", "noscript", "svg",
    ] {
        if let Ok(re) = Regex::new(&format!(r"(?is)<{tag}[^>]*>.*?</{tag}>")) {
            text = re.replace_all(&text, "").to_string();
        }
    }

    // Remove HTML comments
    if let Ok(re) = Regex::new(r"(?s)<!--.*?-->") {
        text = re.replace_all(&text, "").to_string();
    }

    // Convert headings
    for level in 1..=6 {
        let prefix = "#".repeat(level);
        if let Ok(re) = Regex::new(&format!(r"(?i)<h{level}[^>]*>(.*?)</h{level}>")) {
            text = re
                .replace_all(&text, |caps: &regex::Captures| {
                    format!("\n\n{prefix} {}\n\n", strip_tags(&caps[1]))
                })
                .to_string();
        }
    }

    // Convert pre/code blocks
    if let Ok(re) = Regex::new(r"(?is)<pre[^>]*>\s*<code[^>]*>(.*?)</code>\s*</pre>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("\n\n```\n{}\n```\n\n", decode_entities(&caps[1]))
            })
            .to_string();
    }
    if let Ok(re) = Regex::new(r"(?is)<pre[^>]*>(.*?)</pre>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("\n\n```\n{}\n```\n\n", decode_entities(&caps[1]))
            })
            .to_string();
    }

    // Convert inline code
    if let Ok(re) = Regex::new(r"(?i)<code[^>]*>(.*?)</code>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("`{}`", decode_entities(&caps[1]))
            })
            .to_string();
    }

    // Convert links
    if let Ok(re) = Regex::new(r#"(?i)<a[^>]*href="([^"]*)"[^>]*>(.*?)</a>"#) {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                let href = &caps[1];
                let link_text = strip_tags(&caps[2]);
                if link_text.is_empty() || href.starts_with('#') || href.starts_with("javascript:")
                {
                    link_text
                } else {
                    format!("[{link_text}]({href})")
                }
            })
            .to_string();
    }

    // Convert images
    if let Ok(re) = Regex::new(r#"(?i)<img[^>]*alt="([^"]*)"[^>]*src="([^"]*)"[^>]*/?>"#) {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("![{}]({})", &caps[1], &caps[2])
            })
            .to_string();
    }

    // Convert emphasis
    if let Ok(re) = Regex::new(r"(?i)<(?:strong|b)>(.*?)</(?:strong|b)>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("**{}**", strip_tags(&caps[1]))
            })
            .to_string();
    }
    if let Ok(re) = Regex::new(r"(?i)<(?:em|i)>(.*?)</(?:em|i)>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("*{}*", strip_tags(&caps[1]))
            })
            .to_string();
    }

    // Convert list items
    if let Ok(re) = Regex::new(r"(?i)<li[^>]*>(.*?)</li>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("\n- {}", strip_tags(&caps[1]).trim())
            })
            .to_string();
    }

    // Convert blockquotes
    if let Ok(re) = Regex::new(r"(?is)<blockquote[^>]*>(.*?)</blockquote>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                let content = strip_tags(&caps[1]);
                let quoted: Vec<String> = content.lines().map(|l| format!("> {l}")).collect();
                format!("\n\n{}\n\n", quoted.join("\n"))
            })
            .to_string();
    }

    // Convert <br> and <hr>
    if let Ok(re) = Regex::new(r"(?i)<br\s*/?>") {
        text = re.replace_all(&text, "\n").to_string();
    }
    if let Ok(re) = Regex::new(r"(?i)<hr\s*/?>") {
        text = re.replace_all(&text, "\n\n---\n\n").to_string();
    }

    // Convert paragraphs and divs to double newlines
    if let Ok(re) = Regex::new(r"(?i)</?(?:p|div|section|article|main)[^>]*>") {
        text = re.replace_all(&text, "\n\n").to_string();
    }

    // Remove remaining HTML tags
    text = strip_tags(&text);

    // Decode HTML entities
    text = decode_entities(&text);

    // Clean up whitespace: collapse multiple blank lines, trim lines
    if let Ok(re) = Regex::new(r"\n{3,}") {
        text = re.replace_all(&text, "\n\n").to_string();
    }
    // Collapse multiple spaces within lines
    if let Ok(re) = Regex::new(r"[ \t]{2,}") {
        text = re.replace_all(&text, " ").to_string();
    }

    text.trim().to_string()
}

/// Strip all HTML tags from text.
pub(super) fn strip_tags(html: &str) -> String {
    if let Ok(re) = Regex::new(r"<[^>]*>") {
        re.replace_all(html, "").to_string()
    } else {
        html.to_string()
    }
}

/// Decode common HTML entities.
pub(super) fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&copy;", "©")
        .replace("&reg;", "®")
        .replace("&trade;", "™")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_markdown_headings() {
        let html = "<h1>Title</h1><h2>Section</h2><h3>Subsection</h3>";
        let md = html_to_markdown(html);
        assert!(md.contains("# Title"));
        assert!(md.contains("## Section"));
        assert!(md.contains("### Subsection"));
    }

    #[test]
    fn test_html_to_markdown_paragraphs() {
        let html = "<p>First paragraph.</p><p>Second paragraph.</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("First paragraph."));
        assert!(md.contains("Second paragraph."));
    }

    #[test]
    fn test_html_to_markdown_links() {
        let html = r#"<a href="https://example.com">Click here</a>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[Click here](https://example.com)"));
    }

    #[test]
    fn test_html_to_markdown_emphasis() {
        let html = "<strong>bold</strong> and <em>italic</em>";
        let md = html_to_markdown(html);
        assert!(md.contains("**bold**"));
        assert!(md.contains("*italic*"));
    }

    #[test]
    fn test_html_to_markdown_lists() {
        let html = "<ul><li>Item 1</li><li>Item 2</li><li>Item 3</li></ul>";
        let md = html_to_markdown(html);
        assert!(md.contains("- Item 1"));
        assert!(md.contains("- Item 2"));
        assert!(md.contains("- Item 3"));
    }

    #[test]
    fn test_html_to_markdown_code_blocks() {
        let html = "<pre><code>fn main() {\n    println!(\"hello\");\n}</code></pre>";
        let md = html_to_markdown(html);
        assert!(md.contains("```"));
        assert!(md.contains("fn main()"));
    }

    #[test]
    fn test_html_to_markdown_inline_code() {
        let html = "Use <code>cargo build</code> to compile.";
        let md = html_to_markdown(html);
        assert!(md.contains("`cargo build`"));
    }

    #[test]
    fn test_html_to_markdown_strips_scripts() {
        let html = "<p>Content</p><script>alert('xss')</script><p>More content</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("Content"));
        assert!(md.contains("More content"));
        assert!(!md.contains("alert"));
        assert!(!md.contains("script"));
    }

    #[test]
    fn test_html_to_markdown_strips_styles() {
        let html = "<style>.foo { color: red; }</style><p>Visible text</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("Visible text"));
        assert!(!md.contains("color"));
    }

    #[test]
    fn test_html_to_markdown_strips_nav() {
        let html = "<nav><a href='/'>Home</a></nav><main><p>Main content</p></main>";
        let md = html_to_markdown(html);
        assert!(md.contains("Main content"));
        assert!(!md.contains("Home"));
    }

    #[test]
    fn test_html_to_markdown_entities() {
        let html = "<p>A &amp; B &lt; C &gt; D &quot;E&quot; F&#39;s</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("A & B < C > D \"E\" F's"));
    }

    #[test]
    fn test_html_to_markdown_blockquote() {
        let html = "<blockquote>Important quote</blockquote>";
        let md = html_to_markdown(html);
        assert!(md.contains("> Important quote"));
    }

    #[test]
    fn test_html_to_markdown_hr() {
        let html = "<p>Before</p><hr><p>After</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("---"));
    }

    #[test]
    fn test_html_to_markdown_full_page() {
        let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title><style>body { margin: 0; }</style></head>
<body>
<nav><a href="/">Home</a></nav>
<main>
<h1>Welcome</h1>
<p>This is a <strong>test</strong> page with <a href="https://example.com">a link</a>.</p>
<h2>Code Example</h2>
<pre><code>println!("hello");</code></pre>
<ul>
<li>Item one</li>
<li>Item two</li>
</ul>
</main>
<footer>Copyright 2024</footer>
<script>console.log('hidden');</script>
</body>
</html>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("# Welcome"));
        assert!(md.contains("**test**"));
        assert!(md.contains("[a link](https://example.com)"));
        assert!(md.contains("```"));
        assert!(md.contains("- Item one"));
        assert!(!md.contains("console.log"));
        assert!(!md.contains("margin: 0"));
        assert!(!md.contains("<nav>"));
    }

    #[test]
    fn test_strip_tags() {
        assert_eq!(strip_tags("<p>hello <b>world</b></p>"), "hello world");
        assert_eq!(strip_tags("no tags"), "no tags");
    }

    #[test]
    fn test_decode_entities() {
        assert_eq!(decode_entities("&amp; &lt; &gt;"), "& < >");
        assert_eq!(decode_entities("&mdash; &ndash;"), "— –");
    }

    #[test]
    fn test_non_html_passthrough() {
        let text = "This is plain text, not HTML.";
        let md = html_to_markdown(text);
        assert_eq!(md, text);
    }
}
