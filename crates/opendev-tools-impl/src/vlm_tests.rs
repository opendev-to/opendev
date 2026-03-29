use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_vlm_missing_prompt() {
    let tool = VlmTool;
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("prompt is required"));
}

#[tokio::test]
async fn test_vlm_missing_image() {
    let tool = VlmTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("prompt", serde_json::json!("Describe this image"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("image_path or image_url"));
}

#[tokio::test]
async fn test_vlm_invalid_image_url() {
    let tool = VlmTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("prompt", serde_json::json!("Describe this")),
        ("image_url", serde_json::json!("ftp://invalid.com/img.png")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Invalid image URL"));
}

#[tokio::test]
async fn test_vlm_image_not_found() {
    let tool = VlmTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("prompt", serde_json::json!("Describe this")),
        (
            "image_path",
            serde_json::json!("/tmp/nonexistent_image.png"),
        ),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_vlm_unsupported_provider() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let tool = VlmTool;
    let ctx = ToolContext::new(&dir_path);

    let img_path = dir_path.join("test_vlm.png");
    std::fs::write(&img_path, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

    let args = make_args(&[
        ("prompt", serde_json::json!("Describe")),
        ("image_path", serde_json::json!(img_path.to_string_lossy())),
        ("provider", serde_json::json!("google")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Unsupported provider"));
}

#[tokio::test]
async fn test_vlm_anthropic_not_supported() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let tool = VlmTool;
    let ctx = ToolContext::new(&dir_path);

    let img_path = dir_path.join("test_vlm_anthropic.png");
    std::fs::write(&img_path, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

    let args = make_args(&[
        ("prompt", serde_json::json!("Describe")),
        ("image_path", serde_json::json!(img_path.to_string_lossy())),
        ("provider", serde_json::json!("anthropic")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("different request format"));
}

#[tokio::test]
async fn test_vlm_no_api_key() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let tool = VlmTool;
    let ctx = ToolContext::new(&dir_path);

    let img_path = dir_path.join("test_vlm_nokey.png");
    std::fs::write(&img_path, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

    // Ensure no API key is set
    // SAFETY: test-only; ensures no API key interferes with the test.
    unsafe { std::env::remove_var("OPENAI_API_KEY") };

    let args = make_args(&[
        ("prompt", serde_json::json!("Describe")),
        ("image_path", serde_json::json!(img_path.to_string_lossy())),
        ("provider", serde_json::json!("openai")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("API key not found"));
}
