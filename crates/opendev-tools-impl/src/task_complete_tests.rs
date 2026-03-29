use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_task_complete_success() {
    let tool = TaskCompleteTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[(
        "result",
        serde_json::json!("Implemented the feature and added tests"),
    )]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("success"));
    assert!(output.contains("Implemented the feature"));
    assert_eq!(
        result.metadata.get("_completion"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        result.metadata.get("status"),
        Some(&serde_json::json!("success"))
    );
}

#[tokio::test]
async fn test_task_complete_partial() {
    let tool = TaskCompleteTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("result", serde_json::json!("Completed 3 of 5 items")),
        ("status", serde_json::json!("partial")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("partial"));
}

#[tokio::test]
async fn test_task_complete_failed() {
    let tool = TaskCompleteTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("result", serde_json::json!("Could not resolve the issue")),
        ("status", serde_json::json!("failed")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("failed"));
}

#[tokio::test]
async fn test_task_complete_missing_result() {
    let tool = TaskCompleteTool;
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Result is required"));
}

#[tokio::test]
async fn test_task_complete_empty_result() {
    let tool = TaskCompleteTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("result", serde_json::json!("   "))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Result is required"));
}

#[tokio::test]
async fn test_task_complete_invalid_status() {
    let tool = TaskCompleteTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("result", serde_json::json!("Done")),
        ("status", serde_json::json!("cancelled")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Invalid status"));
}

#[tokio::test]
async fn test_task_complete_default_status() {
    let tool = TaskCompleteTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("result", serde_json::json!("All done"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert_eq!(
        result.metadata.get("status"),
        Some(&serde_json::json!("success"))
    );
}

#[tokio::test]
async fn test_task_complete_trims_result() {
    let tool = TaskCompleteTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("result", serde_json::json!("  Trimmed result  "))]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert_eq!(
        result.metadata.get("summary"),
        Some(&serde_json::json!("Trimmed result"))
    );
}
