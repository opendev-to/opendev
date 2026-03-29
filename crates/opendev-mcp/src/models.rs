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
#[path = "models_tests.rs"]
mod tests;
