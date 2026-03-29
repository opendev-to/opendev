use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_present_plan_missing_path() {
    let tool = PresentPlanTool::new();
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("plan_file_path is required"));
}

#[tokio::test]
async fn test_present_plan_file_not_found() {
    let tool = PresentPlanTool::new();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[(
        "plan_file_path",
        serde_json::json!("/tmp/nonexistent_plan.md"),
    )]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_present_plan_empty_file() {
    let tool = PresentPlanTool::new();
    let ctx = ToolContext::new("/tmp");

    let path = std::env::temp_dir().join("test_empty_plan_rs.md");
    std::fs::write(&path, "").unwrap();

    let args = make_args(&[("plan_file_path", serde_json::json!(path.to_string_lossy()))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("empty"));

    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn test_present_plan_too_short() {
    let tool = PresentPlanTool::new();
    let ctx = ToolContext::new("/tmp");

    let path = std::env::temp_dir().join("test_short_plan_rs.md");
    std::fs::write(&path, "A short plan.").unwrap();

    let args = make_args(&[("plan_file_path", serde_json::json!(path.to_string_lossy()))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("too short"));

    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn test_present_plan_missing_delimiter() {
    let tool = PresentPlanTool::new();
    let ctx = ToolContext::new("/tmp");

    let path = std::env::temp_dir().join("test_no_delimiter_plan_rs.md");
    let content = "x".repeat(200);
    std::fs::write(&path, &content).unwrap();

    let args = make_args(&[("plan_file_path", serde_json::json!(path.to_string_lossy()))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("---BEGIN PLAN---"));

    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn test_present_plan_valid_no_auto_todos() {
    let todo_mgr = Arc::new(Mutex::new(TodoManager::new()));
    let tool = PresentPlanTool::with_todo_manager(Arc::clone(&todo_mgr));
    let ctx = ToolContext::new("/tmp");

    let path = std::env::temp_dir().join("test_valid_plan_todos_rs.md");
    let content = format!(
        "# Plan\n\n---BEGIN PLAN---\n\n## Implementation Steps\n\n\
         1. First step\n2. Second step\n3. Third step\n\n\
         ## Verification\n\n1. Run tests\n2. Check lint\n\n\
         ---END PLAN---\n\n{}\n",
        "Additional details. ".repeat(10)
    );
    std::fs::write(&path, &content).unwrap();

    let args = make_args(&[("plan_file_path", serde_json::json!(path.to_string_lossy()))]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "Error: {:?}", result.error);
    assert!(
        result
            .output
            .as_ref()
            .unwrap()
            .contains("Proceed with implementation")
    );
    assert_eq!(
        result.metadata.get("plan_approved"),
        Some(&serde_json::json!(true))
    );

    // Verify NO todos were auto-created (LLM handles via write_todos)
    let mgr = todo_mgr.lock().unwrap();
    assert_eq!(mgr.total(), 0);

    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn test_present_plan_valid_no_todo_manager() {
    let tool = PresentPlanTool::new();
    let ctx = ToolContext::new("/tmp");

    let path = std::env::temp_dir().join("test_valid_plan_no_todo_rs.md");
    let content = format!(
        "# Plan\n\n---BEGIN PLAN---\n\n## Implementation Steps\n\n\
         1. First step\n2. Second step\n3. Third step\n\n\
         ## Verification\n\n1. Run tests\n2. Check lint\n\n\
         ---END PLAN---\n\n{}\n",
        "Additional details. ".repeat(10)
    );
    std::fs::write(&path, &content).unwrap();

    let args = make_args(&[("plan_file_path", serde_json::json!(path.to_string_lossy()))]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "Error: {:?}", result.error);

    std::fs::remove_file(&path).ok();
}

#[test]
fn test_expand_tilde() {
    let expanded = expand_tilde("~/test/plan.md");
    assert!(!expanded.to_string_lossy().starts_with('~'));

    let no_tilde = expand_tilde("/absolute/path");
    assert_eq!(no_tilde, PathBuf::from("/absolute/path"));
}
