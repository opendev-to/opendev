use super::*;

#[test]
fn test_workspace_symbol_tool_metadata() {
    let lsp = Arc::new(Mutex::new(LspWrapper::new(None)));
    let tool = WorkspaceSymbolTool::new(lsp);
    assert_eq!(tool.name(), "workspace_symbol");
    assert!(tool.description().contains("workspace/symbol"));

    let schema = tool.parameter_schema();
    let props = schema.get("properties").unwrap();
    assert!(props.get("query").is_some());
    assert!(props.get("file_hint").is_some());

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert_eq!(required[0].as_str().unwrap(), "query");
}

#[tokio::test]
async fn test_workspace_symbol_tool_missing_query() {
    let lsp = Arc::new(Mutex::new(LspWrapper::new(None)));
    let tool = WorkspaceSymbolTool::new(lsp);
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Missing required parameter"));
}
