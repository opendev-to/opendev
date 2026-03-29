use super::*;

fn make_tool() -> (TodoTool, Arc<Mutex<opendev_runtime::TodoManager>>) {
    let mgr = Arc::new(Mutex::new(opendev_runtime::TodoManager::from_steps(&[
        "Step A".into(),
        "Step B".into(),
        "Step C".into(),
    ])));
    let tool = TodoTool::new(Arc::clone(&mgr));
    (tool, mgr)
}

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_list() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(make_args(&[("action", serde_json::json!("list"))]), &ctx)
        .await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("0/3 done"));
    assert!(output.contains("Step A"));
}

#[tokio::test]
async fn test_list_empty() {
    let mgr = Arc::new(Mutex::new(opendev_runtime::TodoManager::new()));
    let tool = TodoTool::new(mgr);
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(make_args(&[("action", serde_json::json!("list"))]), &ctx)
        .await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("No todos"));
}

#[tokio::test]
async fn test_start() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[
                ("action", serde_json::json!("start")),
                ("id", serde_json::json!(1)),
            ]),
            &ctx,
        )
        .await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("in-progress"));
}

#[tokio::test]
async fn test_complete() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[
                ("action", serde_json::json!("complete")),
                ("id", serde_json::json!(1)),
            ]),
            &ctx,
        )
        .await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("completed"));
}

#[tokio::test]
async fn test_complete_all() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    for id in 1..=3 {
        tool.execute(
            make_args(&[
                ("action", serde_json::json!("complete")),
                ("id", serde_json::json!(id)),
            ]),
            &ctx,
        )
        .await;
    }
    let result = tool
        .execute(make_args(&[("action", serde_json::json!("list"))]), &ctx)
        .await;
    assert!(result.output.unwrap().contains("3/3 done"));
}

#[tokio::test]
async fn test_add() {
    let (tool, mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[
                ("action", serde_json::json!("add")),
                ("title", serde_json::json!("New step")),
            ]),
            &ctx,
        )
        .await;
    assert!(result.success);
    assert_eq!(mgr.lock().unwrap().total(), 4);
}

#[tokio::test]
async fn test_missing_action() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_unknown_action() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(make_args(&[("action", serde_json::json!("unknown"))]), &ctx)
        .await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_start_nonexistent() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool
        .execute(
            make_args(&[
                ("action", serde_json::json!("start")),
                ("id", serde_json::json!(999)),
            ]),
            &ctx,
        )
        .await;
    assert!(!result.success);
}
