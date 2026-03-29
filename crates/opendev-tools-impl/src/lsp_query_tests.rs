use super::*;

fn make_tool() -> LspQueryTool {
    let lsp = Arc::new(Mutex::new(LspWrapper::new(None)));
    LspQueryTool::new(lsp)
}

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn test_lsp_query_tool_metadata() {
    let tool = make_tool();
    assert_eq!(tool.name(), "lsp_query");
    assert!(tool.description().contains("definition"));
    assert!(tool.description().contains("references"));
    assert!(tool.description().contains("hover"));
    assert!(tool.description().contains("symbols"));

    let schema = tool.parameter_schema();
    let props = schema.get("properties").unwrap();
    assert!(props.get("action").is_some());
    assert!(props.get("file_path").is_some());
    assert!(props.get("line").is_some());
    assert!(props.get("character").is_some());
    assert!(props.get("query").is_some());

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&serde_json::json!("action")));
    assert!(required.contains(&serde_json::json!("file_path")));
}

#[tokio::test]
async fn test_lsp_query_missing_action() {
    let tool = make_tool();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("file_path", serde_json::json!("test.rs"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("action"));
}

#[tokio::test]
async fn test_lsp_query_missing_file_path() {
    let tool = make_tool();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("action", serde_json::json!("definition"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("file_path"));
}

#[tokio::test]
async fn test_lsp_query_unknown_action() {
    let tool = make_tool();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("action", serde_json::json!("invalid")),
        ("file_path", serde_json::json!("test.rs")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Unknown action"));
}

#[tokio::test]
async fn test_lsp_query_definition_missing_position() {
    let tool = make_tool();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("action", serde_json::json!("definition")),
        ("file_path", serde_json::json!("test.rs")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("line"));
}

#[tokio::test]
async fn test_lsp_query_references_missing_position() {
    let tool = make_tool();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("action", serde_json::json!("references")),
        ("file_path", serde_json::json!("test.rs")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("line"));
}

#[tokio::test]
async fn test_lsp_query_hover_missing_position() {
    let tool = make_tool();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("action", serde_json::json!("hover")),
        ("file_path", serde_json::json!("test.rs")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("line"));
}
