use super::*;

#[test]
fn test_bridge_tool_name_prefixed() {
    let schema = McpToolSchema {
        name: "sqlite__query".to_string(),
        description: "Run a SQL query".to_string(),
        parameters: serde_json::json!({"type": "object", "properties": {"sql": {"type": "string"}}}),
        server_name: "sqlite".to_string(),
        original_name: "query".to_string(),
    };
    let manager = Arc::new(McpManager::new(None));
    let tool = McpBridgeTool::from_schema(&schema, manager);

    assert_eq!(tool.name(), "mcp__sqlite__query");
    assert_eq!(tool.description(), "Run a SQL query");
}

#[test]
fn test_bridge_tool_schema() {
    let input_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "File path"}
        },
        "required": ["path"]
    });
    let schema = McpToolSchema {
        name: "fs__read".to_string(),
        description: "Read a file".to_string(),
        parameters: input_schema.clone(),
        server_name: "fs".to_string(),
        original_name: "read".to_string(),
    };
    let manager = Arc::new(McpManager::new(None));
    let tool = McpBridgeTool::from_schema(&schema, manager);

    assert_eq!(tool.parameter_schema(), input_schema);
}

#[test]
fn test_bridge_tool_fallback_schema() {
    let schema = McpToolSchema {
        name: "test__noop".to_string(),
        description: "No-op".to_string(),
        parameters: serde_json::Value::Null,
        server_name: "test".to_string(),
        original_name: "noop".to_string(),
    };
    let manager = Arc::new(McpManager::new(None));
    let tool = McpBridgeTool::from_schema(&schema, manager);

    let ps = tool.parameter_schema();
    assert_eq!(ps["type"], "object");
}
