//! MCP tool bridge: wraps an MCP server tool as a `BaseTool`.
//!
//! Each `McpBridgeTool` instance represents a single tool from a connected
//! MCP server. It stores the tool's metadata (name, description, schema)
//! and holds an `Arc<McpManager>` to dispatch `call_tool` requests.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use opendev_mcp::McpManager;
use opendev_mcp::models::{McpContent, McpToolSchema};
use opendev_tools_core::traits::{BaseTool, ToolContext, ToolResult};

/// A `BaseTool` wrapper around a single MCP server tool.
///
/// The tool name is the namespaced MCP name (e.g., `sqlite__query`),
/// prefixed with `mcp__` for the agent's tool registry.
pub struct McpBridgeTool {
    /// Fully qualified tool name for the registry (e.g., `mcp__sqlite__query`).
    tool_name: String,
    /// Human-readable description.
    tool_description: String,
    /// JSON Schema for the tool's parameters.
    schema: serde_json::Value,
    /// Server name for routing the call.
    server_name: String,
    /// Original tool name on the MCP server.
    original_name: String,
    /// Shared MCP manager for dispatching calls.
    manager: Arc<McpManager>,
}

impl std::fmt::Debug for McpBridgeTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpBridgeTool")
            .field("tool_name", &self.tool_name)
            .field("server_name", &self.server_name)
            .field("original_name", &self.original_name)
            .finish()
    }
}

impl McpBridgeTool {
    /// Create a bridge tool from an MCP tool schema and a shared manager.
    pub fn from_schema(schema: &McpToolSchema, manager: Arc<McpManager>) -> Self {
        Self {
            tool_name: format!("mcp__{}", schema.name),
            tool_description: schema.description.clone(),
            schema: schema.parameters.clone(),
            server_name: schema.server_name.clone(),
            original_name: schema.original_name.clone(),
            manager,
        }
    }
}

#[async_trait]
impl BaseTool for McpBridgeTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.tool_description
    }

    fn parameter_schema(&self) -> serde_json::Value {
        // Return the schema as-is; it's already a JSON Schema object from the MCP server
        if self.schema.is_object() {
            self.schema.clone()
        } else {
            // Fallback: wrap in a minimal object schema
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        }
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let arguments = serde_json::Value::Object(args.into_iter().collect());

        match self
            .manager
            .call_tool(&self.server_name, &self.original_name, arguments)
            .await
        {
            Ok(result) => {
                // Convert MCP content blocks to a single output string
                let output = result
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        McpContent::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                if result.is_error {
                    ToolResult::fail(if output.is_empty() {
                        "MCP tool returned an error".to_string()
                    } else {
                        output
                    })
                } else {
                    ToolResult::ok(if output.is_empty() {
                        "(no output)".to_string()
                    } else {
                        output
                    })
                }
            }
            Err(e) => ToolResult::fail(format!("MCP call failed: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
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
}
