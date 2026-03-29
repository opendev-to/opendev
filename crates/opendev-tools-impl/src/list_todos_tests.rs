use super::*;

fn make_tool() -> (ListTodosTool, Arc<Mutex<TodoManager>>) {
    let mgr = Arc::new(Mutex::new(TodoManager::from_steps(&[
        "Step A".into(),
        "Step B".into(),
    ])));
    let tool = ListTodosTool::new(Arc::clone(&mgr));
    (tool, mgr)
}

#[tokio::test]
async fn test_list_with_items() {
    let (tool, _mgr) = make_tool();
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("0/2 done"));
    assert!(output.contains("Step A"));
}

#[tokio::test]
async fn test_list_empty() {
    let mgr = Arc::new(Mutex::new(TodoManager::new()));
    let tool = ListTodosTool::new(mgr);
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("No todos"));
}
