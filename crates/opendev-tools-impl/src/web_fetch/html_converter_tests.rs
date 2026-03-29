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
