use super::*;

#[test]
fn test_urlencoded() {
    assert_eq!(urlencoded("hello world"), "hello+world");
    assert_eq!(urlencoded("rust+lang"), "rust%2Blang");
    assert_eq!(urlencoded("test"), "test");
}

#[test]
fn test_extract_domain() {
    assert_eq!(
        extract_domain("https://www.example.com/page"),
        Some("example.com".to_string())
    );
    assert_eq!(
        extract_domain("https://docs.rust-lang.org/book/"),
        Some("docs.rust-lang.org".to_string())
    );
    assert_eq!(
        extract_domain("http://localhost:8080/test"),
        Some("localhost".to_string())
    );
    assert_eq!(extract_domain("ftp://files.example.com"), None);
}

#[test]
fn test_filter_by_domain() {
    let results = vec![
        SearchResult {
            title: "Rust".into(),
            url: "https://www.rust-lang.org".into(),
            snippet: "A language".into(),
        },
        SearchResult {
            title: "Go".into(),
            url: "https://golang.org".into(),
            snippet: "Another language".into(),
        },
        SearchResult {
            title: "Docs".into(),
            url: "https://docs.rust-lang.org".into(),
            snippet: "Rust docs".into(),
        },
    ];

    // Allowed filter
    let filtered = filter_by_domain(results.clone(), &["rust-lang.org".to_string()], &[]);
    assert_eq!(filtered.len(), 2); // rust-lang.org and docs.rust-lang.org

    // Blocked filter
    let filtered = filter_by_domain(results.clone(), &[], &["golang.org".to_string()]);
    assert_eq!(filtered.len(), 2); // everything except golang.org
}

#[test]
fn test_strip_html_tags() {
    assert_eq!(strip_html_tags("<b>bold</b> text"), "bold text");
    assert_eq!(strip_html_tags("no tags here"), "no tags here");
    assert_eq!(
        strip_html_tags("<a href=\"x\">link</a> and <em>emphasis</em>"),
        "link and emphasis"
    );
}

#[test]
fn test_html_decode() {
    assert_eq!(html_decode("&amp;"), "&");
    assert_eq!(html_decode("&lt;div&gt;"), "<div>");
    assert_eq!(html_decode("it&#39;s"), "it's");
}

#[test]
fn test_urldecode() {
    assert_eq!(urldecode("hello%20world"), "hello world");
    assert_eq!(
        urldecode("https%3A%2F%2Fexample.com"),
        "https://example.com"
    );
}

#[test]
fn test_extract_redirect_url() {
    let url = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage&rut=abc";
    assert_eq!(
        extract_redirect_url(url),
        Some("https://example.com/page".to_string())
    );
}

#[test]
fn test_parse_ddg_html_basic() {
    let html = r#"
    <div class="result results_links results_links_deep web-result">
        <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Frust-lang.org&rut=abc">Rust Programming Language</a>
        <a class="result__snippet">A systems programming language focused on safety.</a>
    </div>
    "#;

    let results = parse_ddg_html(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Rust Programming Language");
    assert_eq!(results[0].url, "https://rust-lang.org");
    assert!(results[0].snippet.contains("systems programming"));
}
