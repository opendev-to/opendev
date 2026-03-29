use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_ask_user_basic() {
    let tool = AskUserTool::new();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("question", serde_json::json!("What language?"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("What language?"));
    assert_eq!(
        result.metadata.get("requires_input"),
        Some(&serde_json::json!(true))
    );
}

#[tokio::test]
async fn test_ask_user_with_options() {
    let tool = AskUserTool::new();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("question", serde_json::json!("Pick one")),
        ("options", serde_json::json!(["A", "B", "C"])),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let out = result.output.unwrap();
    assert!(out.contains("1. A"));
    assert!(out.contains("2. B"));
    assert!(out.contains("3. C"));
}

#[tokio::test]
async fn test_ask_user_missing_question() {
    let tool = AskUserTool::new();
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_ask_user_with_channel() {
    let (tx, mut rx) = opendev_runtime::ask_user_channel();
    let tool = AskUserTool::new().with_ask_tx(tx);
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("question", serde_json::json!("Pick?")),
        ("options", serde_json::json!(["Rust", "Python"])),
    ]);

    // Spawn a task to answer the question
    tokio::spawn(async move {
        let req = rx.recv().await.unwrap();
        assert_eq!(req.question, "Pick?");
        req.response_tx.send("Rust".into()).unwrap();
    });

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("User answered: Rust"));
}
