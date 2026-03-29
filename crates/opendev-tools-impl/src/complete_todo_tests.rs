use super::*;
use opendev_runtime::TodoStatus;

fn make_tool() -> (CompleteTodoTool, Arc<Mutex<TodoManager>>) {
    let mgr = Arc::new(Mutex::new(TodoManager::from_steps(&[
        "Step A".into(),
        "Step B".into(),
    ])));
    let tool = CompleteTodoTool::new(Arc::clone(&mgr));
    (tool, mgr)
}

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_complete_by_id() {
    let (tool, mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(make_args(&[("id", serde_json::json!("1"))]), &ctx)
        .await;
    assert!(result.success);
    assert_eq!(
        mgr.lock().unwrap().get(1).unwrap().status,
        TodoStatus::Completed
    );
}

#[tokio::test]
async fn test_complete_all_message() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    tool.execute(make_args(&[("id", serde_json::json!("1"))]), &ctx)
        .await;
    let result = tool
        .execute(make_args(&[("id", serde_json::json!("2"))]), &ctx)
        .await;
    assert!(result.output.unwrap().contains("All todos are done"));
}

#[tokio::test]
async fn test_complete_not_found() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(make_args(&[("id", serde_json::json!("999"))]), &ctx)
        .await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_complete_by_numeric_value() {
    let (tool, mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(make_args(&[("id", serde_json::json!(2))]), &ctx)
        .await;
    assert!(result.success);
    assert_eq!(
        mgr.lock().unwrap().get(2).unwrap().status,
        TodoStatus::Completed
    );
}
