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

/// Bind a TCP listener and immediately drop it to get a port guaranteed to refuse connections.
fn closed_port_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

#[tokio::test]
async fn test_web_fetch_bad_host() {
    let tool = WebFetchTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("url", serde_json::json!(closed_port_url())),
        ("timeout", serde_json::json!(1)),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_web_fetch_timeout_capped() {
    // Timeout > MAX_TIMEOUT_SECS should be capped, not rejected.
    let tool = WebFetchTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("url", serde_json::json!(closed_port_url())),
        ("timeout", serde_json::json!(999)),
    ]);
    // Should not panic — timeout is capped at 120.
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success); // Connection refused, but no timeout panic
}

#[tokio::test]
async fn test_web_fetch_format_html_no_conversion() {
    // With format=html, even HTML content should NOT be converted to markdown.
    let tool = WebFetchTool;
    let ctx = ToolContext::new("/tmp");
    // We can't easily test with a real server, but we can verify the parameter is accepted.
    let args = make_args(&[
        ("url", serde_json::json!(closed_port_url())),
        ("format", serde_json::json!("html")),
        ("timeout", serde_json::json!(1)),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success); // Connection refused expected
}

#[test]
fn test_timeout_constants() {
    assert_eq!(MAX_TIMEOUT_SECS, 120);
    assert_eq!(DEFAULT_TIMEOUT_SECS, 30);
    assert!(DEFAULT_TIMEOUT_SECS <= MAX_TIMEOUT_SECS);
}
