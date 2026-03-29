use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_open_browser_missing_url() {
    let tool = OpenBrowserTool;
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("url is required"));
}

#[tokio::test]
async fn test_open_browser_invalid_scheme() {
    let tool = OpenBrowserTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("url", serde_json::json!("ftp://example.com"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("http://"));
}

// Note: We don't test actual browser opening in automated tests
// as it would pop up a browser window.
