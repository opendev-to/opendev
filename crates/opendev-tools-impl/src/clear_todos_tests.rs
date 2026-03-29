use super::*;

#[tokio::test]
async fn test_clear_todos() {
    let mgr = Arc::new(Mutex::new(TodoManager::from_steps(&[
        "A".into(),
        "B".into(),
    ])));
    let tool = ClearTodosTool::new(Arc::clone(&mgr));
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(result.success);
    assert_eq!(mgr.lock().unwrap().total(), 0);
}

#[tokio::test]
async fn test_clear_empty() {
    let mgr = Arc::new(Mutex::new(TodoManager::new()));
    let tool = ClearTodosTool::new(mgr.clone());
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(result.success);
}
