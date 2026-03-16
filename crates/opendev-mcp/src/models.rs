//! MCP protocol types.
//!
//! Defines the data structures used in the Model Context Protocol,
//! including tools, resources, prompts, and their schemas.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An MCP tool exposed by a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Tool name (unique within the server).
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    #[serde(default)]
    pub input_schema: serde_json::Value,
}

/// An MCP resource exposed by a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    /// Resource URI.
    pub uri: String,
    /// Human-readable name.
    #[serde(default)]
    pub name: String,
    /// Description of the resource.
    #[serde(default)]
    pub description: String,
    /// MIME type of the resource content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// An MCP prompt exposed by a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    /// Prompt name.
    pub name: String,
    /// Description of the prompt.
    #[serde(default)]
    pub description: String,
    /// Arguments the prompt accepts.
    #[serde(default)]
    pub arguments: Vec<McpPromptArgument>,
}

/// An argument for an MCP prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    /// Argument name.
    pub name: String,
    /// Description of the argument.
    #[serde(default)]
    pub description: String,
    /// Whether the argument is required.
    #[serde(default)]
    pub required: bool,
}

/// Result of calling an MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    /// Content blocks returned by the tool.
    pub content: Vec<McpContent>,
    /// Whether the tool call resulted in an error.
    #[serde(default)]
    pub is_error: bool,
}

/// Content block in an MCP response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpContent {
    /// Text content.
    Text { text: String },
    /// Image content (base64 encoded).
    Image { data: String, mime_type: String },
    /// Resource reference.
    Resource { uri: String },
}

/// Prompt message from an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptMessage {
    /// Role of the message (user, assistant).
    pub role: String,
    /// Content of the message.
    pub content: McpPromptContent,
}

/// Content of a prompt message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpPromptContent {
    /// Simple text content.
    Text(String),
    /// Structured content block.
    Structured { text: String },
    /// Multiple content blocks.
    Multiple(Vec<McpContent>),
}

/// Result of getting a prompt from an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptResult {
    /// Messages comprising the prompt.
    pub messages: Vec<McpPromptMessage>,
}

/// Summary of an MCP prompt for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptSummary {
    /// Server that provides this prompt.
    pub server_name: String,
    /// Name of the prompt.
    pub prompt_name: String,
    /// Description of the prompt.
    pub description: String,
    /// Argument names the prompt accepts.
    pub arguments: Vec<String>,
    /// Slash command to invoke the prompt (e.g., "/server:prompt").
    pub command: String,
}

/// Information about a connected MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    /// Server name.
    pub name: String,
    /// Whether the server is currently connected.
    pub connected: bool,
    /// Tools provided by the server.
    pub tools: Vec<McpTool>,
    /// Transport type used.
    pub transport: String,
}

/// Tool schema as exposed to the LLM, with server origin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolSchema {
    /// Fully qualified tool name (server_name__tool_name).
    pub name: String,
    /// Tool description.
    pub description: String,
    /// JSON Schema for parameters.
    pub parameters: serde_json::Value,
    /// Server that provides this tool.
    pub server_name: String,
    /// Original tool name on the server.
    pub original_name: String,
}

/// JSON-RPC request for MCP protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
}

/// JSON-RPC notification (no id, no response expected).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
}

/// JSON-RPC response for MCP protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tool_roundtrip() {
        let tool = McpTool {
            name: "read_file".to_string(),
            description: "Read a file from disk".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }),
        };

        let json = serde_json::to_string(&tool).unwrap();
        let deserialized: McpTool = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "read_file");
        assert_eq!(deserialized.description, "Read a file from disk");
    }

    #[test]
    fn test_mcp_content_variants() {
        let text = McpContent::Text {
            text: "hello".to_string(),
        };
        let json = serde_json::to_string(&text).unwrap();
        assert!(json.contains("\"type\":\"text\""));

        let image = McpContent::Image {
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        };
        let json = serde_json::to_string(&image).unwrap();
        assert!(json.contains("\"type\":\"image\""));
    }

    #[test]
    fn test_tool_result_with_error() {
        let result = McpToolResult {
            content: vec![McpContent::Text {
                text: "Something went wrong".to_string(),
            }],
            is_error: true,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: McpToolResult = serde_json::from_str(&json).unwrap();
        assert!(deserialized.is_error);
        assert_eq!(deserialized.content.len(), 1);
    }

    #[test]
    fn test_jsonrpc_request() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "tools/list".to_string(),
            params: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"tools/list\""));
        assert!(!json.contains("params"));
    }

    #[test]
    fn test_jsonrpc_request_with_params() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), serde_json::json!("test-prompt"));
        params.insert("arguments".to_string(), serde_json::json!({"key": "value"}));

        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 42,
            method: "prompts/get".to_string(),
            params: Some(params),
        };

        let json = serde_json::to_string(&req).unwrap();
        let rt: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.id, 42);
        assert_eq!(rt.method, "prompts/get");
        let p = rt.params.unwrap();
        assert_eq!(p["name"], "test-prompt");
    }

    #[test]
    fn test_jsonrpc_response_success() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(1),
            result: Some(serde_json::json!({"tools": []})),
            error: None,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("error"));
        let rt: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert!(rt.error.is_none());
        assert!(rt.result.is_some());
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(1),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: "Method not found".to_string(),
                data: None,
            }),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let rt: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert!(rt.result.is_none());
        let err = rt.error.unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }

    #[test]
    fn test_jsonrpc_notification() {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "notifications/tools/list_changed".to_string(),
            params: None,
        };

        let json = serde_json::to_string(&notif).unwrap();
        assert!(!json.contains("\"id\""));
        let rt: JsonRpcNotification = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.method, "notifications/tools/list_changed");
    }

    #[test]
    fn test_mcp_resource_roundtrip() {
        let resource = McpResource {
            uri: "file:///tmp/test.txt".to_string(),
            name: "test.txt".to_string(),
            description: "A test file".to_string(),
            mime_type: Some("text/plain".to_string()),
        };

        let json = serde_json::to_string(&resource).unwrap();
        let rt: McpResource = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.uri, "file:///tmp/test.txt");
        assert_eq!(rt.mime_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn test_mcp_resource_without_mime() {
        let json = r#"{"uri":"git://repo","name":"repo","description":"A repo"}"#;
        let resource: McpResource = serde_json::from_str(json).unwrap();
        assert!(resource.mime_type.is_none());
    }

    #[test]
    fn test_mcp_prompt_summary() {
        let summary = McpPromptSummary {
            server_name: "my-server".to_string(),
            prompt_name: "code-review".to_string(),
            description: "Review code changes".to_string(),
            arguments: vec!["file".to_string(), "language".to_string()],
            command: "/my-server:code-review".to_string(),
        };

        let json = serde_json::to_string(&summary).unwrap();
        let rt: McpPromptSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.server_name, "my-server");
        assert_eq!(rt.command, "/my-server:code-review");
        assert_eq!(rt.arguments.len(), 2);
    }

    #[test]
    fn test_mcp_prompt_result() {
        let result = McpPromptResult {
            messages: vec![McpPromptMessage {
                role: "user".to_string(),
                content: McpPromptContent::Text("Review this code".to_string()),
            }],
        };

        let json = serde_json::to_string(&result).unwrap();
        let rt: McpPromptResult = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.messages.len(), 1);
        assert_eq!(rt.messages[0].role, "user");
    }

    #[test]
    fn test_mcp_tool_schema_namespacing() {
        let schema = McpToolSchema {
            name: "my_server__read_file".to_string(),
            description: "Read a file".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            server_name: "my_server".to_string(),
            original_name: "read_file".to_string(),
        };

        assert!(schema.name.contains("__"));
        assert_eq!(schema.original_name, "read_file");
    }

    #[test]
    fn test_mcp_server_info() {
        let info = McpServerInfo {
            name: "test-server".to_string(),
            connected: true,
            tools: vec![McpTool {
                name: "hello".to_string(),
                description: "Say hello".to_string(),
                input_schema: serde_json::json!({}),
            }],
            transport: "stdio".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        let rt: McpServerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.tools.len(), 1);
        assert!(rt.connected);
    }

    #[test]
    fn test_mcp_content_resource_variant() {
        let content = McpContent::Resource {
            uri: "file:///test".to_string(),
        };

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"type\":\"resource\""));
        let rt: McpContent = serde_json::from_str(&json).unwrap();
        match rt {
            McpContent::Resource { uri } => {
                assert_eq!(uri, "file:///test");
            }
            _ => panic!("Expected Resource variant"),
        }
    }

    #[test]
    fn test_mcp_tool_result_success() {
        let result = McpToolResult {
            content: vec![
                McpContent::Text {
                    text: "line 1".to_string(),
                },
                McpContent::Text {
                    text: "line 2".to_string(),
                },
            ],
            is_error: false,
        };

        assert!(!result.is_error);
        assert_eq!(result.content.len(), 2);
    }
}
