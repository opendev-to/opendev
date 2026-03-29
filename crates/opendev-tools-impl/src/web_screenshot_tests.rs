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
    assert_eq!(normalize_url("https:/example.com"), "https://example.com");
}

#[test]
fn test_generate_output_path() {
    let path = generate_output_path("https://example.com/page");
    assert!(path.to_string_lossy().contains("example.com"));
    assert!(path.extension().unwrap() == "png");
}

#[tokio::test]
async fn test_web_screenshot_missing_url() {
    let tool = WebScreenshotTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("action", serde_json::json!("capture"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("url is required"));
}

#[tokio::test]
async fn test_web_screenshot_list() {
    let tool = WebScreenshotTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("action", serde_json::json!("list")),
        ("url", serde_json::json!("unused")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
}

#[tokio::test]
async fn test_web_screenshot_clear() {
    let tool = WebScreenshotTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("action", serde_json::json!("clear")),
        ("url", serde_json::json!("unused")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
}
