use super::*;

fn make_tool() -> (WriteTodosTool, Arc<Mutex<TodoManager>>) {
    let mgr = Arc::new(Mutex::new(TodoManager::new()));
    let tool = WriteTodosTool::new(Arc::clone(&mgr));
    (tool, mgr)
}

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_write_todos_strings() {
    let (tool, mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[("todos", serde_json::json!(["Step A", "Step B", "Step C"]))]),
            &ctx,
        )
        .await;
    assert!(result.success);
    assert_eq!(mgr.lock().unwrap().total(), 3);
}

#[tokio::test]
async fn test_write_todos_objects() {
    let (tool, mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[(
                "todos",
                serde_json::json!([
                    {"content": "First", "status": "in_progress", "activeForm": "Working on first"},
                    {"content": "Second"}
                ]),
            )]),
            &ctx,
        )
        .await;
    assert!(result.success);
    let m = mgr.lock().unwrap();
    assert_eq!(m.total(), 2);
    assert_eq!(m.get(1).unwrap().status, TodoStatus::InProgress);
    assert_eq!(m.get(1).unwrap().active_form, "Working on first");
}

#[tokio::test]
async fn test_write_todos_replaces() {
    let (tool, mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    // Write initial
    tool.execute(make_args(&[("todos", serde_json::json!(["Old"]))]), &ctx)
        .await;
    assert_eq!(mgr.lock().unwrap().total(), 1);

    // Replace
    tool.execute(
        make_args(&[("todos", serde_json::json!(["New A", "New B"]))]),
        &ctx,
    )
    .await;
    assert_eq!(mgr.lock().unwrap().total(), 2);
    assert_eq!(mgr.lock().unwrap().get(1).unwrap().title, "New A");
}

#[tokio::test]
async fn test_write_todos_missing_arg() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_write_todos_with_children() {
    let (tool, mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[(
                "todos",
                serde_json::json!([
                    {
                        "content": "Implement auth",
                        "activeForm": "Implementing auth",
                        "children": ["Add login endpoint", "Add token validation"]
                    },
                    {
                        "content": "Write tests",
                        "activeForm": "Writing tests",
                        "children": ["Unit tests", "Integration tests"]
                    }
                ]),
            )]),
            &ctx,
        )
        .await;
    assert!(result.success);
    let m = mgr.lock().unwrap();
    // Only parent items counted
    assert_eq!(m.total(), 2);
    // Children stored on parents
    assert_eq!(m.get(1).unwrap().children.len(), 2);
    assert_eq!(m.get(1).unwrap().children[0].title, "Add login endpoint");
    assert_eq!(m.get(2).unwrap().children.len(), 2);
    // Children appear in result output
    let output = result.output.as_deref().unwrap_or("");
    assert!(output.contains("Add login endpoint"));
    assert!(output.contains("Integration tests"));
}

#[tokio::test]
async fn test_write_todos_skips_empty_content() {
    let (tool, mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[(
                "todos",
                serde_json::json!([
                    "",
                    "Valid item",
                    {"content": "  ", "status": "in_progress"},
                    {"content": "Another valid"}
                ]),
            )]),
            &ctx,
        )
        .await;
    assert!(result.success);
    let m = mgr.lock().unwrap();
    assert_eq!(m.total(), 2, "Empty items should be filtered out");
    assert_eq!(m.get(1).unwrap().title, "Valid item");
    assert_eq!(m.get(2).unwrap().title, "Another valid");
}

#[tokio::test]
async fn test_write_todos_children_bypass_status_only() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    // Write initial without children
    tool.execute(
        make_args(&[("todos", serde_json::json!(["Auth", "Tests"]))]),
        &ctx,
    )
    .await;

    // Write same titles but with children — should NOT use status-only path
    let result = tool
        .execute(
            make_args(&[(
                "todos",
                serde_json::json!([
                    {"content": "Auth", "children": ["Sub-step A"]},
                    {"content": "Tests", "children": ["Sub-step B"]}
                ]),
            )]),
            &ctx,
        )
        .await;
    assert!(result.success);
    let output = result.output.as_deref().unwrap_or("");
    assert!(output.contains("Created 2 todo(s)"));
    assert!(output.contains("Sub-step A"));
}
