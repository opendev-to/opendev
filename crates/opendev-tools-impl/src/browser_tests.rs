use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn test_normalize_url() {
    assert_eq!(normalize_url("example.com"), "https://example.com");
    assert_eq!(normalize_url("https://example.com"), "https://example.com");
    assert_eq!(normalize_url("http://example.com"), "http://example.com");
    assert_eq!(normalize_url("https:/example.com"), "https://example.com");
}

#[test]
fn test_extract_title() {
    assert_eq!(
        extract_title("<html><head><title>My Page</title></head></html>"),
        Some("My Page".to_string())
    );
    assert_eq!(
        extract_title("<html><head><title>Rust &amp; Go</title></head></html>"),
        Some("Rust & Go".to_string())
    );
    assert_eq!(extract_title("<html><body>no title</body></html>"), None);
}

#[test]
fn test_extract_visible_text() {
    let html = "<html><head><style>.x{}</style></head>\
                 <body><p>Hello</p><script>var x=1;</script><p>World</p></body></html>";
    let text = extract_visible_text(html);
    assert!(text.contains("Hello"));
    assert!(text.contains("World"));
    assert!(!text.contains("var x"));
    assert!(!text.contains(".x{}"));
}

#[tokio::test]
async fn test_browser_missing_action() {
    let tool = BrowserTool;
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("action is required"));
}

#[tokio::test]
async fn test_browser_unknown_action() {
    let tool = BrowserTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("action", serde_json::json!("destroy"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Unknown browser action"));
}

#[tokio::test]
async fn test_browser_navigate_missing_url() {
    let tool = BrowserTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("action", serde_json::json!("navigate"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("URL is required"));
}

#[tokio::test]
async fn test_browser_click_missing_selector() {
    let tool = BrowserTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("action", serde_json::json!("click"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("CSS selector is required"));
}

#[tokio::test]
async fn test_browser_type_missing_value() {
    let tool = BrowserTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("action", serde_json::json!("type")),
        ("target", serde_json::json!("#input")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("value (text) is required"));
}

#[tokio::test]
async fn test_browser_evaluate_missing_js() {
    let tool = BrowserTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("action", serde_json::json!("evaluate"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("JavaScript expression"));
}

#[tokio::test]
async fn test_browser_tabs_list() {
    let tool = BrowserTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("action", serde_json::json!("tabs_list"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("No browser context"));
}
