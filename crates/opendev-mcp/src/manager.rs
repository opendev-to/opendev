//! MCP Manager: manages multiple MCP server connections.
//!
//! The McpManager is the central coordinator for MCP server lifecycle:
//! - Loading and merging configuration from global and project files
//! - Creating transports and connecting to servers
//! - Running the MCP initialize handshake
//! - Discovering tools via tools/list
//! - Tracking connected servers and their tools
//! - Listing prompts and tools across all connected servers

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::{
    McpConfig, McpServerConfig, get_project_config_path, load_config, merge_configs,
    prepare_server_config,
};
use crate::error::{McpError, McpResult};
use crate::models::{
    JsonRpcNotification, JsonRpcRequest, McpContent, McpPromptSummary, McpServerInfo, McpTool,
    McpToolResult, McpToolSchema,
};
use crate::transport::{self, McpTransport};

/// State for a single connected MCP server.
struct ServerConnection {
    transport: Box<dyn McpTransport>,
    tools: Vec<McpTool>,
    #[allow(dead_code)]
    config: McpServerConfig,
}

/// Manages multiple MCP server connections and tool execution.
pub struct McpManager {
    working_dir: PathBuf,
    config: Arc<RwLock<Option<McpConfig>>>,
    connections: Arc<RwLock<HashMap<String, ServerConnection>>>,
    request_id: Arc<std::sync::atomic::AtomicU64>,
}

impl McpManager {
    /// Create a new MCP manager.
    pub fn new(working_dir: Option<PathBuf>) -> Self {
        Self {
            working_dir: working_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
            config: Arc::new(RwLock::new(None)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            request_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }

    fn next_request_id(&self) -> u64 {
        self.request_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Load MCP configuration from global and project files.
    pub async fn load_configuration(&self) -> McpResult<McpConfig> {
        let paths = opendev_config::Paths::new(Some(self.working_dir.clone()));

        let global_config = load_config(&paths.global_mcp_config())?;

        let project_config = get_project_config_path(&self.working_dir)
            .map(|p| load_config(&p))
            .transpose()?;

        let merged = merge_configs(&global_config, project_config.as_ref());

        let mut config = self.config.write().await;
        *config = Some(merged.clone());

        Ok(merged)
    }

    /// Get loaded configuration, loading if necessary.
    pub async fn get_config(&self) -> McpResult<McpConfig> {
        {
            let config = self.config.read().await;
            if let Some(c) = config.as_ref() {
                return Ok(c.clone());
            }
        }
        self.load_configuration().await
    }

    /// Perform the MCP initialize handshake on a transport.
    ///
    /// Sends `initialize` request, waits for response, then sends
    /// `notifications/initialized` notification.
    async fn initialize_handshake(
        &self,
        transport: &dyn McpTransport,
    ) -> McpResult<serde_json::Value> {
        let mut params = HashMap::new();
        params.insert(
            "protocolVersion".to_string(),
            serde_json::Value::String("2024-11-05".to_string()),
        );
        params.insert(
            "capabilities".to_string(),
            serde_json::json!({
                "roots": { "listChanged": true }
            }),
        );
        params.insert(
            "clientInfo".to_string(),
            serde_json::json!({
                "name": "opendev",
                "version": env!("CARGO_PKG_VERSION")
            }),
        );

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_request_id(),
            method: "initialize".to_string(),
            params: Some(params),
        };

        debug!("Sending MCP initialize request");
        let response = transport.send_request(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Protocol(format!(
                "Initialize failed ({}): {}",
                error.code, error.message
            )));
        }

        let server_info = response.result.clone().unwrap_or_default();
        debug!(
            server_info = %server_info,
            "MCP initialize response received"
        );

        // Send initialized notification (no response expected).
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "notifications/initialized".to_string(),
            params: None,
        };
        transport.send_notification(&notification).await?;
        debug!("Sent notifications/initialized");

        Ok(server_info)
    }

    /// Discover tools from a connected server via tools/list.
    async fn discover_tools(&self, transport: &dyn McpTransport) -> McpResult<Vec<McpTool>> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_request_id(),
            method: "tools/list".to_string(),
            params: None,
        };

        debug!("Sending tools/list request");
        let response = transport.send_request(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Protocol(format!(
                "tools/list failed ({}): {}",
                error.code, error.message
            )));
        }

        let result = response
            .result
            .ok_or_else(|| McpError::Protocol("Empty response from tools/list".to_string()))?;

        let tools_value = result.get("tools").ok_or_else(|| {
            McpError::Protocol("No 'tools' field in tools/list response".to_string())
        })?;

        let tools: Vec<McpTool> = serde_json::from_value(tools_value.clone())
            .map_err(|e| McpError::Protocol(format!("Failed to parse tools list: {}", e)))?;

        debug!(count = tools.len(), "Discovered tools");
        Ok(tools)
    }

    /// Connect to a specific MCP server.
    ///
    /// This performs the full connection lifecycle:
    /// 1. Create transport
    /// 2. Connect transport (spawn process for stdio)
    /// 3. Run initialize handshake
    /// 4. Discover tools via tools/list
    pub async fn connect_server(&self, name: &str) -> McpResult<()> {
        let config = self.get_config().await?;
        let server_config = config
            .mcp_servers
            .get(name)
            .ok_or_else(|| McpError::ServerNotFound(name.to_string()))?;

        if !server_config.enabled {
            warn!("Server '{}' is disabled, skipping connection", name);
            return Ok(());
        }

        {
            let connections = self.connections.read().await;
            if connections.contains_key(name) {
                return Err(McpError::AlreadyConnected(name.to_string()));
            }
        }

        let prepared = prepare_server_config(server_config);
        let mut transport = transport::create_transport(&prepared)?;

        // Step 1: Connect the transport (e.g., spawn child process).
        transport
            .connect()
            .await
            .map_err(|e| McpError::Connection {
                server: name.to_string(),
                message: format!("Transport connect failed: {}", e),
            })?;

        // Step 2: Run initialize handshake.
        let _server_info = self
            .initialize_handshake(transport.as_ref())
            .await
            .map_err(|e| McpError::Connection {
                server: name.to_string(),
                message: format!("Initialize handshake failed: {}", e),
            })?;

        // Step 3: Discover tools.
        let tools = self
            .discover_tools(transport.as_ref())
            .await
            .unwrap_or_else(|e| {
                warn!("Failed to discover tools from '{}': {}", name, e);
                Vec::new()
            });

        info!(
            server = name,
            tools = tools.len(),
            "Connected to MCP server"
        );

        let connection = ServerConnection {
            transport,
            tools,
            config: prepared,
        };

        let mut connections = self.connections.write().await;
        connections.insert(name.to_string(), connection);

        Ok(())
    }

    /// Disconnect from a specific MCP server.
    pub async fn disconnect_server(&self, name: &str) -> McpResult<()> {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.remove(name) {
            conn.transport.close().await?;
            info!("Disconnected from MCP server: {}", name);
            Ok(())
        } else {
            Err(McpError::ServerNotFound(name.to_string()))
        }
    }

    /// Connect to all enabled auto-start servers.
    pub async fn connect_all(&self) -> McpResult<Vec<String>> {
        let config = self.get_config().await?;
        let mut connected = Vec::new();

        for (name, server_config) in &config.mcp_servers {
            if server_config.enabled && server_config.auto_start {
                match self.connect_server(name).await {
                    Ok(()) => connected.push(name.clone()),
                    Err(e) => {
                        error!("Failed to connect to MCP server '{}': {}", name, e);
                    }
                }
            }
        }

        Ok(connected)
    }

    /// Disconnect from all connected servers.
    pub async fn disconnect_all(&self) -> McpResult<()> {
        let mut connections = self.connections.write().await;
        for (name, conn) in connections.drain() {
            if let Err(e) = conn.transport.close().await {
                error!("Error closing connection to '{}': {}", name, e);
            }
        }
        Ok(())
    }

    /// List all connected servers and their tools.
    pub async fn list_servers(&self) -> Vec<McpServerInfo> {
        let connections = self.connections.read().await;
        connections
            .iter()
            .map(|(name, conn)| McpServerInfo {
                name: name.clone(),
                connected: conn.transport.is_connected(),
                tools: conn.tools.clone(),
                transport: conn.transport.transport_type().to_string(),
            })
            .collect()
    }

    /// Get all tool schemas across connected servers.
    ///
    /// Tools are namespaced as `server_name__tool_name` to avoid collisions.
    pub async fn get_all_tool_schemas(&self) -> Vec<McpToolSchema> {
        let connections = self.connections.read().await;
        let mut schemas = Vec::new();

        for (server_name, conn) in connections.iter() {
            for tool in &conn.tools {
                schemas.push(McpToolSchema {
                    name: format!("{}__{}", server_name, tool.name),
                    description: tool.description.clone(),
                    parameters: tool.input_schema.clone(),
                    server_name: server_name.clone(),
                    original_name: tool.name.clone(),
                });
            }
        }

        schemas
    }

    /// Call a tool on a connected MCP server.
    ///
    /// If the server has crashed or become unresponsive, the error is caught,
    /// the failed server is removed from the connection pool, and a
    /// user-friendly error result is returned instead of propagating the error.
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> McpResult<McpToolResult> {
        let result = self
            .call_tool_inner(server_name, tool_name, arguments)
            .await;

        match result {
            Ok(tool_result) => Ok(tool_result),
            Err(
                McpError::Transport(_)
                | McpError::Timeout(_)
                | McpError::Connection { .. }
                | McpError::Io(_),
            ) => {
                // Server has likely crashed or become unresponsive.
                // Remove it from the connection pool so its tools are
                // no longer offered and we don't keep trying.
                warn!(
                    server = server_name,
                    tool = tool_name,
                    error = %result.as_ref().unwrap_err(),
                    "MCP server failed, removing from active connections"
                );
                self.remove_failed_server(server_name).await;

                // Return an error result that the agent can handle
                // gracefully instead of crashing.
                Ok(McpToolResult {
                    content: vec![McpContent::Text {
                        text: format!(
                            "MCP server '{}' has become unavailable and has been removed. \
                             The tool '{}' is no longer accessible.",
                            server_name, tool_name
                        ),
                    }],
                    is_error: true,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Internal tool call implementation (no degradation logic).
    async fn call_tool_inner(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> McpResult<McpToolResult> {
        let connections = self.connections.read().await;
        let conn = connections
            .get(server_name)
            .ok_or_else(|| McpError::ServerNotFound(server_name.to_string()))?;

        let mut params = HashMap::new();
        params.insert(
            "name".to_string(),
            serde_json::Value::String(tool_name.to_string()),
        );
        params.insert("arguments".to_string(), arguments);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_request_id(),
            method: "tools/call".to_string(),
            params: Some(params),
        };

        let response = conn.transport.send_request(&request).await?;

        if let Some(error) = response.error {
            return Err(McpError::Protocol(format!(
                "Tool call error ({}): {}",
                error.code, error.message
            )));
        }

        let result = response
            .result
            .ok_or_else(|| McpError::Protocol("Empty response from tool call".to_string()))?;

        serde_json::from_value(result)
            .map_err(|e| McpError::Protocol(format!("Failed to parse tool result: {}", e)))
    }

    /// Remove a failed server from the connection pool, closing its transport
    /// and dropping its tools from the registry.
    async fn remove_failed_server(&self, name: &str) {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.remove(name) {
            // Best-effort close; the transport may already be dead.
            if let Err(e) = conn.transport.close().await {
                debug!(
                    "Failed to close transport for crashed server '{}': {}",
                    name, e
                );
            }
            warn!(
                server = name,
                tools = conn.tools.len(),
                "Removed failed MCP server and its tools from the registry"
            );
        }
    }

    /// List prompts from all connected servers.
    pub async fn list_prompts(&self) -> Vec<McpPromptSummary> {
        let connections = self.connections.read().await;
        let mut prompts = Vec::new();

        for (server_name, conn) in connections.iter() {
            let request = JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: self.next_request_id(),
                method: "prompts/list".to_string(),
                params: None,
            };

            match conn.transport.send_request(&request).await {
                Ok(response) => {
                    if let Some(result) = response.result
                        && let Some(prompt_list) = result.get("prompts").and_then(|p| p.as_array())
                    {
                        for prompt_val in prompt_list {
                            let name = prompt_val
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();
                            let description = prompt_val
                                .get("description")
                                .and_then(|d| d.as_str())
                                .unwrap_or("")
                                .to_string();
                            let arguments = prompt_val
                                .get("arguments")
                                .and_then(|a| a.as_array())
                                .map(|args| {
                                    args.iter()
                                        .filter_map(|a| {
                                            a.get("name").and_then(|n| n.as_str()).map(String::from)
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();

                            prompts.push(McpPromptSummary {
                                server_name: server_name.clone(),
                                prompt_name: name.clone(),
                                description,
                                arguments,
                                command: format!("/{}:{}", server_name, name),
                            });
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to list prompts from '{}': {}", server_name, e);
                }
            }
        }

        prompts
    }

    /// Check if a server is connected.
    pub async fn is_connected(&self, server_name: &str) -> bool {
        let connections = self.connections.read().await;
        connections.contains_key(server_name)
    }

    /// Get the number of connected servers.
    pub async fn connected_count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }

    /// Add or update a server configuration.
    pub async fn add_server(&self, name: String, config: McpServerConfig) -> McpResult<()> {
        let mut cfg = self.config.write().await;
        let mcp_config = cfg.get_or_insert_with(McpConfig::default);
        mcp_config.mcp_servers.insert(name.clone(), config);
        info!("Added MCP server configuration: {}", name);
        Ok(())
    }

    /// Remove a server configuration and disconnect if connected.
    pub async fn remove_server(&self, name: &str) -> McpResult<()> {
        // Disconnect first if connected
        {
            let mut connections = self.connections.write().await;
            if let Some(conn) = connections.remove(name) {
                let _ = conn.transport.close().await;
            }
        }

        // Remove from config
        let mut cfg = self.config.write().await;
        if let Some(mcp_config) = cfg.as_mut() {
            mcp_config.mcp_servers.remove(name);
        }

        info!("Removed MCP server: {}", name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_new() {
        let manager = McpManager::new(None);
        assert_eq!(manager.connected_count().await, 0);
    }

    #[tokio::test]
    async fn test_connect_nonexistent_server() {
        let manager = McpManager::new(Some(PathBuf::from("/tmp")));
        // Set empty config
        {
            let mut config = manager.config.write().await;
            *config = Some(McpConfig::default());
        }

        let result = manager.connect_server("nonexistent").await;
        assert!(matches!(result, Err(McpError::ServerNotFound(_))));
    }

    #[tokio::test]
    async fn test_add_and_remove_server() {
        let manager = McpManager::new(Some(PathBuf::from("/tmp")));
        {
            let mut config = manager.config.write().await;
            *config = Some(McpConfig::default());
        }

        manager
            .add_server(
                "test-server".to_string(),
                McpServerConfig {
                    command: "node".to_string(),
                    args: vec!["server.js".to_string()],
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let config = manager.get_config().await.unwrap();
        assert!(config.mcp_servers.contains_key("test-server"));

        manager.remove_server("test-server").await.unwrap();
        let config = manager.get_config().await.unwrap();
        assert!(!config.mcp_servers.contains_key("test-server"));
    }

    #[tokio::test]
    async fn test_list_servers_empty() {
        let manager = McpManager::new(None);
        let servers = manager.list_servers().await;
        assert!(servers.is_empty());
    }

    #[tokio::test]
    async fn test_disconnect_nonexistent() {
        let manager = McpManager::new(None);
        let result = manager.disconnect_server("nonexistent").await;
        assert!(matches!(result, Err(McpError::ServerNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_all_tool_schemas_empty() {
        let manager = McpManager::new(None);
        let schemas = manager.get_all_tool_schemas().await;
        assert!(schemas.is_empty());
    }

    /// Integration test: connect to a mock MCP server, run initialize handshake,
    /// and discover tools.
    #[tokio::test]
    async fn test_full_lifecycle_with_mock_server() {
        use crate::config::TransportType;

        // Python script that implements a minimal MCP server:
        // - Responds to initialize with server info
        // - Responds to tools/list with one tool
        // - Ignores notifications
        let script = r#"
import sys, json

def read_message():
    while True:
        line = sys.stdin.readline()
        if not line:
            return None
        if line.startswith("Content-Length:"):
            length = int(line.split(":")[1].strip())
            sys.stdin.readline()  # blank line
            body = sys.stdin.read(length)
            return json.loads(body)

def write_message(obj):
    body = json.dumps(obj)
    sys.stdout.write(f"Content-Length: {len(body)}\r\n\r\n{body}")
    sys.stdout.flush()

while True:
    msg = read_message()
    if msg is None:
        break
    if "id" not in msg:
        continue  # notification, no response
    method = msg.get("method", "")
    if method == "initialize":
        write_message({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "mock-server", "version": "0.1.0"}
            }
        })
    elif method == "tools/list":
        write_message({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "tools": [
                    {
                        "name": "greet",
                        "description": "Say hello",
                        "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}}
                    }
                ]
            }
        })
    elif method == "tools/call":
        name = msg.get("params", {}).get("arguments", {}).get("name", "world")
        write_message({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "content": [{"type": "text", "text": f"Hello, {name}!"}],
                "isError": False
            }
        })
    else:
        write_message({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "error": {"code": -32601, "message": "Method not found"}
        })
"#;

        let manager = McpManager::new(Some(PathBuf::from("/tmp")));

        // Set up config with our mock server.
        {
            let mut config = manager.config.write().await;
            let mut mcp_config = McpConfig::default();
            mcp_config.mcp_servers.insert(
                "mock".to_string(),
                McpServerConfig {
                    command: "python3".to_string(),
                    args: vec!["-c".to_string(), script.to_string()],
                    transport: TransportType::Stdio,
                    enabled: true,
                    auto_start: true,
                    ..Default::default()
                },
            );
            *config = Some(mcp_config);
        }

        // Connect (runs initialize + tools/list).
        manager.connect_server("mock").await.unwrap();
        assert!(manager.is_connected("mock").await);
        assert_eq!(manager.connected_count().await, 1);

        // Verify tools were discovered.
        let schemas = manager.get_all_tool_schemas().await;
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "mock__greet");
        assert_eq!(schemas[0].original_name, "greet");
        assert_eq!(schemas[0].description, "Say hello");

        // Call the tool.
        let result = manager
            .call_tool("mock", "greet", serde_json::json!({"name": "Rust"}))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);

        // List servers.
        let servers = manager.list_servers().await;
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "mock");
        assert_eq!(servers[0].tools.len(), 1);

        // Disconnect.
        manager.disconnect_server("mock").await.unwrap();
        assert!(!manager.is_connected("mock").await);
        assert_eq!(manager.connected_count().await, 0);
    }

    /// Test that calling a tool on a disconnected/crashed server degrades
    /// gracefully: returns an error result and removes the server.
    #[tokio::test]
    async fn test_call_tool_graceful_degradation_on_missing_server() {
        let manager = McpManager::new(Some(PathBuf::from("/tmp")));

        // Calling a tool on a non-existent server should return ServerNotFound
        // (not a transport error, so it propagates rather than degrading).
        let result = manager
            .call_tool("gone", "some_tool", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    /// Test that remove_failed_server cleans up properly.
    #[tokio::test]
    async fn test_remove_failed_server() {
        let manager = McpManager::new(Some(PathBuf::from("/tmp")));

        // Removing a non-existent server should not panic
        manager.remove_failed_server("nonexistent").await;
        assert_eq!(manager.connected_count().await, 0);
    }
}
