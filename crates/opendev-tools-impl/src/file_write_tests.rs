use super::*;
use tempfile::TempDir;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_write_file_basic() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");

    let tool = FileWriteTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("content", serde_json::json!("hello\nworld\n")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert_eq!(
        std::fs::read_to_string(&file_path).unwrap(),
        "hello\nworld\n"
    );
}

#[tokio::test]
async fn test_write_creates_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("a/b/c/test.txt");

    let tool = FileWriteTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("content", serde_json::json!("nested content")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert_eq!(
        std::fs::read_to_string(&file_path).unwrap(),
        "nested content"
    );
}

#[tokio::test]
async fn test_write_no_create_dirs() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("nonexistent/test.txt");

    let tool = FileWriteTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("content", serde_json::json!("content")),
        ("create_dirs", serde_json::json!(false)),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("does not exist"));
}

#[tokio::test]
async fn test_write_overwrites_existing() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "old content").unwrap();

    let tool = FileWriteTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("content", serde_json::json!("new content")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "new content");
}
