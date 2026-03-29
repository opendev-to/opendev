use super::*;
use opendev_runtime::TodoStatus;

fn make_tool() -> (UpdateTodoTool, Arc<Mutex<TodoManager>>) {
    let mgr = Arc::new(Mutex::new(TodoManager::from_steps(&[
        "Step A".into(),
        "Step B".into(),
        "Step C".into(),
    ])));
    let tool = UpdateTodoTool::new(Arc::clone(&mgr));
    (tool, mgr)
}

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_update_status_by_number() {
    let (tool, mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[
                ("id", serde_json::json!("2")),
                ("status", serde_json::json!("in_progress")),
            ]),
            &ctx,
        )
        .await;
    assert!(result.success);
    assert_eq!(
        mgr.lock().unwrap().get(2).unwrap().status,
        TodoStatus::InProgress
    );
}

#[tokio::test]
async fn test_update_status_by_todo_dash_n() {
    let (tool, mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[
                ("id", serde_json::json!("todo-1")),
                ("status", serde_json::json!("doing")),
            ]),
            &ctx,
        )
        .await;
    assert!(result.success);
    assert_eq!(
        mgr.lock().unwrap().get(1).unwrap().status,
        TodoStatus::InProgress
    );
}

#[tokio::test]
async fn test_update_not_found() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[
                ("id", serde_json::json!("999")),
                ("status", serde_json::json!("done")),
            ]),
            &ctx,
        )
        .await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_update_invalid_status() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[
                ("id", serde_json::json!("1")),
                ("status", serde_json::json!("invalid")),
            ]),
            &ctx,
        )
        .await;
    assert!(!result.success);
}
