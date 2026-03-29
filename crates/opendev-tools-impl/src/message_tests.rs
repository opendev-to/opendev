use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn test_build_payload_slack_text() {
    let payload = build_payload("slack", "Hello!", "text");
    assert_eq!(payload, serde_json::json!({"text": "Hello!"}));
}

#[test]
fn test_build_payload_slack_markdown() {
    let payload = build_payload("slack", "*Bold*", "markdown");
    assert!(payload.get("blocks").is_some());
}

#[test]
fn test_build_payload_discord() {
    let payload = build_payload("discord", "Hello Discord", "text");
    assert_eq!(payload, serde_json::json!({"content": "Hello Discord"}));
}

#[test]
fn test_build_payload_generic() {
    let payload = build_payload("webhook", "data", "text");
    assert_eq!(payload.get("text").unwrap(), "data");
    assert_eq!(payload.get("format").unwrap(), "text");
}

#[tokio::test]
async fn test_message_missing_channel() {
    let tool = MessageTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("message", serde_json::json!("hello"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("channel is required"));
}

#[tokio::test]
async fn test_message_missing_message() {
    let tool = MessageTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("channel", serde_json::json!("slack"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("message is required"));
}

#[tokio::test]
async fn test_message_no_webhook_url() {
    let tool = MessageTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("channel", serde_json::json!("slack")),
        ("message", serde_json::json!("hello")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("No webhook URL"));
}

#[tokio::test]
async fn test_message_invalid_webhook_url() {
    let tool = MessageTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("channel", serde_json::json!("slack")),
        ("message", serde_json::json!("hello")),
        ("target", serde_json::json!("not-a-url")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("http://"));
}

#[tokio::test]
async fn test_message_bad_webhook_host() {
    let tool = MessageTool;
    let ctx = ToolContext::new("/tmp");
    // Bind and drop a listener to get a port guaranteed to refuse connections instantly.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let args = make_args(&[
        ("channel", serde_json::json!("slack")),
        ("message", serde_json::json!("hello")),
        (
            "target",
            serde_json::json!(format!("http://127.0.0.1:{port}/webhook")),
        ),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
}
