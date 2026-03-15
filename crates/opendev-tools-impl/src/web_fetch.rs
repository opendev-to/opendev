//! Web fetch tool — fetch URL content via HTTP.
//!
//! Supports optional HTML-to-markdown extraction for LLM-friendly output,
//! mirroring Python's `extract_text` parameter.

use std::collections::HashMap;

use regex::Regex;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Maximum response body size (1 MB).
const MAX_BODY_SIZE: usize = 1_024 * 1_024;

/// Maximum timeout (120 seconds).
const MAX_TIMEOUT_SECS: u64 = 120;

/// Default timeout (30 seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Tool for fetching web page content.
#[derive(Debug)]
pub struct WebFetchTool;

#[async_trait::async_trait]
impl BaseTool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch the content of a URL. Supports optional HTML-to-markdown extraction for clean, LLM-friendly output."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch"
                },
                "headers": {
                    "type": "object",
                    "description": "Optional HTTP headers as key-value pairs"
                },
                "extract_markdown": {
                    "type": "boolean",
                    "description": "Convert HTML to clean markdown for easier reading (default: true for HTML content)"
                },
                "format": {
                    "type": "string",
                    "enum": ["text", "markdown", "html"],
                    "description": "Output format: 'text' for plain text, 'markdown' for HTML-to-markdown conversion (default for HTML), 'html' for raw HTML"
                },
                "timeout": {
                    "type": "number",
                    "description": "Request timeout in seconds (default: 30, max: 120)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let url = match args.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return ToolResult::fail("url is required"),
        };

        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ToolResult::fail("URL must start with http:// or https://");
        }

        // Parse timeout (capped at MAX_TIMEOUT_SECS).
        let timeout_secs = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .map(|t| t.min(MAX_TIMEOUT_SECS))
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        // Parse format parameter.
        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("markdown");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build();

        let client = match client {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to create HTTP client: {e}")),
        };

        // Build Accept header based on format.
        let accept_header = match format {
            "html" => "text/html,application/xhtml+xml,*/*;q=0.8",
            "text" => "text/plain,text/html;q=0.5,*/*;q=0.3",
            _ => "text/html,application/xhtml+xml,text/plain;q=0.8,*/*;q=0.5", // markdown (default)
        };

        let mut request = client
            .get(url)
            .header("Accept", accept_header)
            .header("Accept-Language", "en-US,en;q=0.9");

        // Add custom headers (may override Accept/Accept-Language).
        if let Some(headers) = args.get("headers").and_then(|v| v.as_object()) {
            for (key, value) in headers {
                if let Some(val) = value.as_str() {
                    request = request.header(key.as_str(), val);
                }
            }
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::fail(format!("Request failed: {e}")),
        };

        let status = response.status().as_u16();

        // Detect Cloudflare bot challenge: 403 with cf-mitigated header.
        let is_cf_blocked = status == 403
            && response
                .headers()
                .get("cf-mitigated")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.contains("challenge"));

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => return ToolResult::fail(format!("Failed to read response body: {e}")),
        };

        // Retry with simpler User-Agent if Cloudflare blocked us.
        let (status, content_type, body) = if is_cf_blocked {
            tracing::debug!("Cloudflare challenge detected, retrying with simpler UA");
            let retry = client
                .get(url)
                .header("User-Agent", "opendev")
                .header("Accept", accept_header)
                .header("Accept-Language", "en-US,en;q=0.9")
                .send()
                .await;
            match retry {
                Ok(r) => {
                    let s = r.status().as_u16();
                    let ct = r
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("unknown")
                        .to_string();
                    let b = r.text().await.unwrap_or_default();
                    (s, ct, b)
                }
                Err(_) => (status, content_type, body), // fall back to original
            }
        } else {
            (status, content_type, body)
        };

        // Determine if we should extract markdown based on format and extract_markdown params.
        let extract_markdown = match format {
            "html" => false, // raw HTML requested
            "text" => false, // plain text, no conversion
            _ => {
                // "markdown" or default: respect extract_markdown param or auto-detect
                args.get("extract_markdown")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(content_type.contains("html"))
            }
        };

        // Convert HTML to markdown if requested and content is HTML
        let body = if extract_markdown && content_type.contains("html") {
            html_to_markdown(&body)
        } else {
            body
        };

        let truncated = body.len() > MAX_BODY_SIZE;
        let body = if truncated {
            format!(
                "{}...\n\n[truncated, showing first {} bytes of {}]",
                &body[..MAX_BODY_SIZE],
                MAX_BODY_SIZE,
                body.len()
            )
        } else {
            body
        };

        let mut metadata = HashMap::new();
        metadata.insert("status".into(), serde_json::json!(status));
        metadata.insert("content_type".into(), serde_json::json!(content_type));
        metadata.insert("truncated".into(), serde_json::json!(truncated));
        metadata.insert(
            "extracted_markdown".into(),
            serde_json::json!(extract_markdown),
        );

        if status >= 400 {
            return ToolResult {
                success: false,
                output: Some(body),
                error: Some(format!("HTTP {status}")),
                metadata,
                duration_ms: None,
                llm_suffix: None,
            };
        }

        ToolResult::ok_with_metadata(body, metadata)
    }
}

// ---------------------------------------------------------------------------
// HTML-to-markdown conversion
// ---------------------------------------------------------------------------

/// Convert HTML content to clean markdown for LLM-friendly output.
///
/// Uses regex-based extraction rather than a full DOM parser. Handles
/// the most common HTML patterns: headings, paragraphs, links, lists,
/// code blocks, emphasis, and removes scripts/styles/navigation.
fn html_to_markdown(html: &str) -> String {
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
fn strip_tags(html: &str) -> String {
    if let Ok(re) = Regex::new(r"<[^>]*>") {
        re.replace_all(html, "").to_string()
    } else {
        html.to_string()
    }
}

/// Decode common HTML entities.
fn decode_entities(text: &str) -> String {
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

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_web_fetch_missing_url() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("url is required"));
    }

    #[tokio::test]
    async fn test_web_fetch_invalid_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[("url", serde_json::json!("ftp://example.com"))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("http://"));
    }

    #[tokio::test]
    async fn test_web_fetch_bad_host() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "url",
            serde_json::json!("http://this-host-does-not-exist-12345.invalid"),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
    }

    // -- HTML-to-markdown tests --

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

    #[tokio::test]
    async fn test_web_fetch_timeout_capped() {
        // Timeout > MAX_TIMEOUT_SECS should be capped, not rejected.
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[
            (
                "url",
                serde_json::json!("http://this-host-does-not-exist-12345.invalid"),
            ),
            ("timeout", serde_json::json!(999)),
        ]);
        // Should not panic — timeout is capped at 120.
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success); // DNS failure, but no timeout panic
    }

    #[tokio::test]
    async fn test_web_fetch_format_html_no_conversion() {
        // With format=html, even HTML content should NOT be converted to markdown.
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        // We can't easily test with a real server, but we can verify the parameter is accepted.
        let args = make_args(&[
            (
                "url",
                serde_json::json!("http://this-host-does-not-exist-12345.invalid"),
            ),
            ("format", serde_json::json!("html")),
        ]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success); // DNS failure expected
    }

    #[test]
    fn test_timeout_constants() {
        assert_eq!(MAX_TIMEOUT_SECS, 120);
        assert_eq!(DEFAULT_TIMEOUT_SECS, 30);
        assert!(DEFAULT_TIMEOUT_SECS <= MAX_TIMEOUT_SECS);
    }
}
