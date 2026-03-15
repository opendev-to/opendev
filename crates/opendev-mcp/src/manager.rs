//! MCP Manager: manages multiple MCP server connections.
//!
//! The McpManager is the central coordinator for MCP server lifecycle:
//! - Loading and merging configuration from global and project files
//! - Creating transports and connecting to servers
//! - Running the MCP initialize handshake
//! - Discovering tools via tools/list
//! - Tracking connected servers and their tools
//! - Health monitoring with periodic heartbeat pings
//! - Auto-restart on crash with exponential backoff
//! - Tool schema caching with change notification support
//! - Graceful degradation when servers become unhealthy

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::{
    McpConfig, McpOAuthConfig, McpServerConfig, get_project_config_path, load_config,
    merge_configs, prepare_server_config,
};
use crate::error::{McpError, McpResult};
use crate::models::{
    JsonRpcNotification, JsonRpcRequest, McpContent, McpPromptResult, McpPromptSummary, McpResource,
    McpServerInfo, McpTool, McpToolResult, McpToolSchema,
};
use crate::transport::{self, McpTransport};

/// Default health check interval in seconds.
const DEFAULT_HEALTH_CHECK_INTERVAL_SECS: u64 = 30;

/// Number of consecutive health check failures before marking unhealthy.
const HEALTH_CHECK_FAILURE_THRESHOLD: u32 = 3;

/// Maximum number of restart attempts before marking permanently failed.
const MAX_RESTART_ATTEMPTS: u32 = 5;

/// Maximum backoff duration in seconds for restart attempts.
const MAX_BACKOFF_SECS: u64 = 60;

/// Sanitize a server or tool name for use in namespaced tool identifiers.
///
/// Replaces any character that is not alphanumeric, underscore, or hyphen with `_`.
/// This prevents issues with special characters in tool names that could confuse
/// the LLM or break JSON schemas.
fn sanitize_mcp_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Health status of a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerHealthStatus {
    /// Server is healthy and responding to pings.
    Healthy,
    /// Server has failed some health checks but is still within threshold.
    Degraded,
    /// Server has failed enough health checks to be marked unhealthy.
    Unhealthy,
    /// Server has exceeded max restart attempts and is permanently failed.
    PermanentlyFailed,
}

/// Health tracking state for a server.
#[derive(Debug, Clone)]
pub struct ServerHealthState {
    /// Current health status.
    pub status: ServerHealthStatus,
    /// Number of consecutive failed health checks.
    pub consecutive_failures: u32,
    /// Number of restart attempts made.
    pub restart_attempts: u32,
    /// Whether tools have been removed from the registry due to unhealthy status.
    pub tools_removed: bool,
}

impl Default for ServerHealthState {
    fn default() -> Self {
        Self {
            status: ServerHealthStatus::Healthy,
            consecutive_failures: 0,
            restart_attempts: 0,
            tools_removed: false,
        }
    }
}

impl ServerHealthState {
    /// Calculate the next backoff duration for restart attempts.
    pub fn next_backoff_secs(&self) -> u64 {
        let backoff = 1u64 << self.restart_attempts.min(6);
        backoff.min(MAX_BACKOFF_SECS)
    }
}

/// Cached tool schema data for a server.
#[derive(Debug, Clone)]
struct ToolSchemaCache {
    /// The cached tools from the last tools/list response.
    tools: Vec<McpTool>,
    /// Whether the cache has been invalidated (e.g., by a tools/changed notification).
    invalidated: bool,
}

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
    /// Health check interval in seconds. Set to 0 to disable.
    health_check_interval_secs: u64,
    /// Health state for each server (by name).
    health_states: Arc<RwLock<HashMap<String, ServerHealthState>>>,
    /// Cached tool schemas per server.
    tool_schema_cache: Arc<RwLock<HashMap<String, ToolSchemaCache>>>,
    /// Handle for the background health check task.
    health_check_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

/// Lightweight handle for notification listeners to refresh tools.
///
/// Contains only the Arc fields needed for `handle_tools_changed`,
/// avoiding a full McpManager clone.
struct NotificationHandle {
    connections: Arc<RwLock<HashMap<String, ServerConnection>>>,
    tool_schema_cache: Arc<RwLock<HashMap<String, ToolSchemaCache>>>,
    request_id: Arc<std::sync::atomic::AtomicU64>,
}

impl NotificationHandle {
    /// Invalidate cache and re-discover tools for a server.
    async fn handle_tools_changed(&self, server_name: &str) {
        info!(
            server = server_name,
            "Received tools/changed notification, refreshing tools"
        );

        // Invalidate the cache.
        {
            let mut cache = self.tool_schema_cache.write().await;
            if let Some(entry) = cache.get_mut(server_name) {
                entry.invalidated = true;
            }
        }

        // Re-discover tools from the live transport.
        let connections = self.connections.read().await;
        let Some(conn) = connections.get(server_name) else {
            warn!(server = server_name, "Server not found for tools refresh");
            return;
        };

        let request_id = self
            .request_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let request = crate::models::JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: request_id,
            method: "tools/list".to_string(),
            params: None,
        };

        match conn.transport.send_request(&request).await {
            Ok(response) => {
                if let Some(result) = response.result
                    && let Some(tools_val) = result.get("tools")
                    && let Ok(tools) =
                        serde_json::from_value::<Vec<McpTool>>(tools_val.clone())
                {
                    // Update cache.
                    let mut cache = self.tool_schema_cache.write().await;
                    cache.insert(
                        server_name.to_string(),
                        ToolSchemaCache {
                            tools: tools.clone(),
                            invalidated: false,
                        },
                    );
                    drop(cache);
                    drop(connections);

                    // Update connection's tool list.
                    let mut conns_write = self.connections.write().await;
                    if let Some(conn) = conns_write.get_mut(server_name) {
                        conn.tools = tools.clone();
                    }

                    info!(
                        server = server_name,
                        tools = tools.len(),
                        "Tools refreshed after tools/changed notification"
                    );
                    return;
                }
                warn!(
                    server = server_name,
                    "Failed to parse tools/list response after notification"
                );
            }
            Err(e) => {
                warn!(
                    server = server_name,
                    error = %e,
                    "Failed to refresh tools after tools/changed notification"
                );
            }
        }
    }
}

impl McpManager {
    /// Create a lightweight handle for notification listener tasks.
    fn clone_for_notifications(&self) -> NotificationHandle {
        NotificationHandle {
            connections: Arc::clone(&self.connections),
            tool_schema_cache: Arc::clone(&self.tool_schema_cache),
            request_id: Arc::clone(&self.request_id),
        }
    }

    /// Create a new MCP manager.
    pub fn new(working_dir: Option<PathBuf>) -> Self {
        Self {
            working_dir: working_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
            config: Arc::new(RwLock::new(None)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            request_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            health_check_interval_secs: DEFAULT_HEALTH_CHECK_INTERVAL_SECS,
            health_states: Arc::new(RwLock::new(HashMap::new())),
            tool_schema_cache: Arc::new(RwLock::new(HashMap::new())),
            health_check_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new MCP manager with a custom health check interval.
    pub fn with_health_check_interval(mut self, interval_secs: u64) -> Self {
        self.health_check_interval_secs = interval_secs;
        self
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

    /// Acquire an OAuth 2.0 access token using the client_credentials grant.
    ///
    /// Performs a POST to the `token_url` with the client credentials and
    /// optional scope. Returns the access token string on success.
    pub async fn acquire_oauth_token(oauth: &McpOAuthConfig) -> McpResult<String> {
        let client = reqwest::Client::new();

        let mut params = vec![
            ("grant_type", "client_credentials".to_string()),
            ("client_id", oauth.client_id.clone()),
            ("client_secret", oauth.client_secret.clone()),
        ];
        if let Some(ref scope) = oauth.scope {
            params.push(("scope", scope.clone()));
        }

        debug!(token_url = %oauth.token_url, "Acquiring OAuth token");

        // Build URL-encoded form body manually (no reqwest `form` feature needed)
        fn simple_url_encode(s: &str) -> String {
            let mut out = String::with_capacity(s.len());
            for b in s.bytes() {
                match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        out.push(b as char);
                    }
                    _ => {
                        out.push_str(&format!("%{:02X}", b));
                    }
                }
            }
            out
        }
        let form_body = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, simple_url_encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let response = client
            .post(&oauth.token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_body)
            .send()
            .await
            .map_err(|e| McpError::Transport(format!("OAuth token request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(McpError::Transport(format!(
                "OAuth token request returned {}: {}",
                status, body
            )));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpError::Transport(format!("OAuth token response parse error: {}", e)))?;

        let token = body
            .get("access_token")
            .and_then(|t| t.as_str())
            .ok_or_else(|| {
                McpError::Transport("OAuth token response missing 'access_token' field".to_string())
            })?;

        info!("Successfully acquired OAuth token");
        Ok(token.to_string())
    }

    /// Connect to a specific MCP server.
    ///
    /// This performs the full connection lifecycle:
    /// 1. Create transport
    /// 2. Connect transport (spawn process for stdio)
    /// 3. Run initialize handshake
    /// 4. Discover tools via tools/list (or use cache)
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

        let mut prepared = prepare_server_config(server_config);

        // If OAuth config is present, acquire a token and inject it as
        // an Authorization header for HTTP/SSE transports.
        if let Some(ref oauth) = prepared.oauth {
            let token = Self::acquire_oauth_token(oauth).await?;
            prepared
                .headers
                .insert("Authorization".to_string(), format!("Bearer {}", token));
        }

        let mut transport = transport::create_transport(&prepared)?;

        let connect_timeout_ms = prepared.effective_timeout_ms();
        let connect_timeout = std::time::Duration::from_millis(connect_timeout_ms);

        // Step 1: Connect the transport (e.g., spawn child process).
        tokio::time::timeout(connect_timeout, transport.connect())
            .await
            .map_err(|_| McpError::Timeout(connect_timeout_ms / 1000))?
            .map_err(|e| McpError::Connection {
                server: name.to_string(),
                message: format!("Transport connect failed: {}", e),
            })?;

        // Step 2: Run initialize handshake.
        let _server_info = tokio::time::timeout(
            connect_timeout,
            self.initialize_handshake(transport.as_ref()),
        )
        .await
        .map_err(|_| McpError::Timeout(connect_timeout_ms / 1000))?
        .map_err(|e| McpError::Connection {
            server: name.to_string(),
            message: format!("Initialize handshake failed: {}", e),
        })?;

        // Step 3: Discover tools (use cache if available and not invalidated).
        let tools = tokio::time::timeout(
            connect_timeout,
            self.get_or_discover_tools(name, transport.as_ref()),
        )
        .await
        .map_err(|_| McpError::Timeout(connect_timeout_ms / 1000))?
        .unwrap_or_else(|e| {
            warn!("Failed to discover tools from '{}': {}", name, e);
            Vec::new()
        });

        info!(
            server = name,
            tools = tools.len(),
            "Connected to MCP server"
        );

        // Take the notification receiver and spawn a listener task
        // that handles server-initiated notifications like tools/changed.
        if let Some(mut notif_rx) = transport.take_notification_receiver().await {
            let server_name = name.to_string();
            let manager = self.clone_for_notifications();
            tokio::spawn(async move {
                while let Some(notif) = notif_rx.recv().await {
                    match notif.method.as_str() {
                        "notifications/tools/list_changed" | "tools/changed" => {
                            manager.handle_tools_changed(&server_name).await;
                        }
                        other => {
                            debug!(
                                server = %server_name,
                                method = other,
                                "Unhandled MCP server notification"
                            );
                        }
                    }
                }
                debug!(server = %server_name, "Notification listener stopped");
            });
        }

        let connection = ServerConnection {
            transport,
            tools,
            config: prepared,
        };

        let mut connections = self.connections.write().await;
        connections.insert(name.to_string(), connection);

        // Initialize health state for this server.
        let mut health_states = self.health_states.write().await;
        health_states.insert(name.to_string(), ServerHealthState::default());

        Ok(())
    }

    /// Get tools from cache or discover them fresh.
    async fn get_or_discover_tools(
        &self,
        server_name: &str,
        transport: &dyn McpTransport,
    ) -> McpResult<Vec<McpTool>> {
        // Check cache first.
        {
            let cache = self.tool_schema_cache.read().await;
            if let Some(cached) = cache.get(server_name)
                && !cached.invalidated
            {
                debug!(
                    server = server_name,
                    tools = cached.tools.len(),
                    "Using cached tool schemas"
                );
                return Ok(cached.tools.clone());
            }
        }

        // Discover fresh tools.
        let tools = self.discover_tools(transport).await?;

        // Update cache.
        let mut cache = self.tool_schema_cache.write().await;
        cache.insert(
            server_name.to_string(),
            ToolSchemaCache {
                tools: tools.clone(),
                invalidated: false,
            },
        );

        Ok(tools)
    }

    /// Invalidate the tool schema cache for a specific server.
    ///
    /// Called when a `tools/changed` notification is received, or when
    /// explicitly requested.
    pub async fn invalidate_tool_cache(&self, server_name: &str) {
        let mut cache = self.tool_schema_cache.write().await;
        if let Some(entry) = cache.get_mut(server_name) {
            entry.invalidated = true;
            debug!(server = server_name, "Tool schema cache invalidated");
        }
    }

    /// Invalidate all tool schema caches.
    pub async fn invalidate_all_tool_caches(&self) {
        let mut cache = self.tool_schema_cache.write().await;
        for entry in cache.values_mut() {
            entry.invalidated = true;
        }
        debug!("All tool schema caches invalidated");
    }

    /// Refresh tools for a server by re-querying tools/list.
    ///
    /// Updates both the cache and the active connection's tool list.
    pub async fn refresh_tools(&self, server_name: &str) -> McpResult<Vec<McpTool>> {
        // First invalidate cache.
        self.invalidate_tool_cache(server_name).await;

        // Then re-discover from the live transport.
        let tools = {
            let connections = self.connections.read().await;
            let conn = connections
                .get(server_name)
                .ok_or_else(|| McpError::ServerNotFound(server_name.to_string()))?;
            self.get_or_discover_tools(server_name, conn.transport.as_ref())
                .await?
        };

        // Update the connection's tool list.
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_name) {
            conn.tools = tools.clone();
        }

        Ok(tools)
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

    /// Disconnect from all connected servers and stop health monitoring.
    pub async fn disconnect_all(&self) -> McpResult<()> {
        self.stop_health_monitoring().await;

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
            let sanitized_server = sanitize_mcp_name(server_name);
            for tool in &conn.tools {
                let sanitized_tool = sanitize_mcp_name(&tool.name);
                schemas.push(McpToolSchema {
                    name: format!("{}__{}", sanitized_server, sanitized_tool),
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

        let timeout_ms = conn.config.effective_timeout_ms();
        let response = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            conn.transport.send_request(&request),
        )
        .await
        .map_err(|_| McpError::Timeout(timeout_ms / 1000))??;

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

    // -----------------------------------------------------------------------
    // Health monitoring (#75) and auto-restart (#77)
    // -----------------------------------------------------------------------

    /// Perform a health check (ping) on a specific server.
    ///
    /// Sends a `ping` JSON-RPC request and checks for a response.
    /// Returns `true` if the server responded, `false` otherwise.
    pub async fn ping_server(&self, server_name: &str) -> bool {
        let connections = self.connections.read().await;
        let conn = match connections.get(server_name) {
            Some(c) => c,
            None => return false,
        };

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_request_id(),
            method: "ping".to_string(),
            params: None,
        };

        match conn.transport.send_request(&request).await {
            Ok(_) => true,
            Err(e) => {
                debug!(
                    server = server_name,
                    error = %e,
                    "Health check ping failed"
                );
                false
            }
        }
    }

    /// Record a health check result for a server.
    ///
    /// Updates the health state and takes appropriate action:
    /// - On success: resets consecutive failure count.
    /// - On failure: increments consecutive failure count.
    /// - When threshold is reached: marks unhealthy, removes tools, attempts restart.
    pub async fn record_health_check(&self, server_name: &str, success: bool) {
        let mut health_states = self.health_states.write().await;
        let state = health_states.entry(server_name.to_string()).or_default();

        if state.status == ServerHealthStatus::PermanentlyFailed {
            return;
        }

        if success {
            state.consecutive_failures = 0;
            if state.status == ServerHealthStatus::Degraded {
                state.status = ServerHealthStatus::Healthy;
                info!(server = server_name, "MCP server health restored");
            }
            return;
        }

        // Failure path.
        state.consecutive_failures += 1;
        debug!(
            server = server_name,
            consecutive_failures = state.consecutive_failures,
            "MCP server health check failed"
        );

        if state.consecutive_failures < HEALTH_CHECK_FAILURE_THRESHOLD {
            state.status = ServerHealthStatus::Degraded;
        } else if !state.tools_removed {
            // Threshold reached: mark unhealthy and remove tools.
            state.status = ServerHealthStatus::Unhealthy;
            state.tools_removed = true;
            warn!(
                server = server_name,
                "MCP server marked unhealthy after {} consecutive failures. \
                 Removing its tools from the active registry.",
                state.consecutive_failures
            );
            // Drop the lock before calling async methods.
            drop(health_states);
            self.remove_failed_server(server_name).await;
        }
    }

    /// Attempt to restart a failed server with exponential backoff.
    ///
    /// Returns `true` if restart succeeded, `false` otherwise.
    pub async fn attempt_restart(&self, server_name: &str) -> bool {
        let backoff_secs;
        {
            let mut health_states = self.health_states.write().await;
            let state = health_states.entry(server_name.to_string()).or_default();

            if state.status == ServerHealthStatus::PermanentlyFailed {
                debug!(
                    server = server_name,
                    "Server permanently failed, skipping restart"
                );
                return false;
            }

            if state.restart_attempts >= MAX_RESTART_ATTEMPTS {
                state.status = ServerHealthStatus::PermanentlyFailed;
                error!(
                    server = server_name,
                    attempts = state.restart_attempts,
                    "MCP server permanently failed after {} restart attempts",
                    MAX_RESTART_ATTEMPTS
                );
                return false;
            }

            backoff_secs = state.next_backoff_secs();
            state.restart_attempts += 1;
            info!(
                server = server_name,
                attempt = state.restart_attempts,
                backoff_secs = backoff_secs,
                "Attempting MCP server restart"
            );
        }

        // Wait for backoff duration.
        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;

        // Try to reconnect.
        match self.connect_server(server_name).await {
            Ok(()) => {
                let mut health_states = self.health_states.write().await;
                if let Some(state) = health_states.get_mut(server_name) {
                    state.status = ServerHealthStatus::Healthy;
                    state.consecutive_failures = 0;
                    state.tools_removed = false;
                    // Note: we keep restart_attempts for history.
                }
                info!(server = server_name, "MCP server restart succeeded");
                true
            }
            Err(e) => {
                warn!(
                    server = server_name,
                    error = %e,
                    "MCP server restart failed"
                );
                // Check if we've exhausted all attempts.
                let mut health_states = self.health_states.write().await;
                if let Some(state) = health_states.get_mut(server_name)
                    && state.restart_attempts >= MAX_RESTART_ATTEMPTS
                {
                    state.status = ServerHealthStatus::PermanentlyFailed;
                    error!(
                        server = server_name,
                        attempts = state.restart_attempts,
                        "MCP server permanently failed after {} restart attempts",
                        MAX_RESTART_ATTEMPTS
                    );
                }
                false
            }
        }
    }

    /// Get the health state of a specific server.
    pub async fn get_health_state(&self, server_name: &str) -> Option<ServerHealthState> {
        let health_states = self.health_states.read().await;
        health_states.get(server_name).cloned()
    }

    /// Get health states for all servers.
    pub async fn get_all_health_states(&self) -> HashMap<String, ServerHealthState> {
        let health_states = self.health_states.read().await;
        health_states.clone()
    }

    /// Start background health monitoring.
    ///
    /// Spawns a task that periodically pings all connected servers.
    /// When a server becomes unhealthy, its tools are removed and a
    /// restart is attempted.
    pub async fn start_health_monitoring(&self) {
        if self.health_check_interval_secs == 0 {
            debug!("Health monitoring disabled (interval = 0)");
            return;
        }

        let interval = std::time::Duration::from_secs(self.health_check_interval_secs);
        let connections = Arc::clone(&self.connections);
        let health_states = Arc::clone(&self.health_states);
        let request_id = Arc::clone(&self.request_id);
        let config = Arc::clone(&self.config);

        // We need a way to perform pings and restarts from the background task.
        // We clone the relevant Arc fields and build a lightweight proxy.
        let tool_cache = Arc::clone(&self.tool_schema_cache);
        let _working_dir = self.working_dir.clone();

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // First tick is immediate; skip it.

            loop {
                ticker.tick().await;

                // Collect current server names.
                let server_names: Vec<String> = {
                    let conns = connections.read().await;
                    conns.keys().cloned().collect()
                };

                // Also check servers that were recently marked unhealthy
                // (not permanently failed) for restart.
                let unhealthy_servers: Vec<String> = {
                    let states = health_states.read().await;
                    states
                        .iter()
                        .filter(|(_, s)| s.status == ServerHealthStatus::Unhealthy)
                        .map(|(name, _)| name.clone())
                        .collect()
                };

                // Ping connected servers.
                for name in &server_names {
                    let ping_ok = {
                        let conns = connections.read().await;
                        if let Some(conn) = conns.get(name.as_str()) {
                            let req = JsonRpcRequest {
                                jsonrpc: "2.0".to_string(),
                                id: request_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                                method: "ping".to_string(),
                                params: None,
                            };
                            conn.transport.send_request(&req).await.is_ok()
                        } else {
                            false
                        }
                    };

                    // Update health state.
                    let mut states = health_states.write().await;
                    let state = states.entry(name.clone()).or_default();

                    if state.status == ServerHealthStatus::PermanentlyFailed {
                        continue;
                    }

                    if ping_ok {
                        state.consecutive_failures = 0;
                        if state.status == ServerHealthStatus::Degraded {
                            state.status = ServerHealthStatus::Healthy;
                            info!(server = %name, "MCP server health restored");
                        }
                    } else {
                        state.consecutive_failures += 1;
                        debug!(
                            server = %name,
                            failures = state.consecutive_failures,
                            "Health check ping failed"
                        );

                        if state.consecutive_failures >= HEALTH_CHECK_FAILURE_THRESHOLD
                            && !state.tools_removed
                        {
                            state.status = ServerHealthStatus::Unhealthy;
                            state.tools_removed = true;
                            warn!(
                                server = %name,
                                "MCP server marked unhealthy, removing tools"
                            );
                            // Remove the server connection.
                            drop(states);
                            let mut conns = connections.write().await;
                            if let Some(conn) = conns.remove(name.as_str())
                                && let Err(e) = conn.transport.close().await
                            {
                                debug!("Error closing unhealthy server '{}': {}", name, e);
                            }
                        } else if state.consecutive_failures < HEALTH_CHECK_FAILURE_THRESHOLD {
                            state.status = ServerHealthStatus::Degraded;
                        }
                    }
                }

                // Attempt restart for unhealthy servers.
                for name in &unhealthy_servers {
                    let should_restart = {
                        let states = health_states.read().await;
                        if let Some(state) = states.get(name.as_str()) {
                            state.status == ServerHealthStatus::Unhealthy
                                && state.restart_attempts < MAX_RESTART_ATTEMPTS
                        } else {
                            false
                        }
                    };

                    if !should_restart {
                        continue;
                    }

                    let backoff_secs;
                    {
                        let mut states = health_states.write().await;
                        let state = states.entry(name.clone()).or_default();
                        backoff_secs = state.next_backoff_secs();
                        state.restart_attempts += 1;
                        info!(
                            server = %name,
                            attempt = state.restart_attempts,
                            backoff_secs,
                            "Attempting background restart"
                        );
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;

                    // Try to reconnect using the stored config.
                    let restart_ok = {
                        let cfg = config.read().await;
                        if let Some(mcp_config) = cfg.as_ref() {
                            if let Some(server_config) = mcp_config.mcp_servers.get(name.as_str()) {
                                let prepared = prepare_server_config(server_config);
                                match transport::create_transport(&prepared) {
                                    Ok(mut t) => {
                                        if t.connect().await.is_ok() {
                                            // Minimal reconnect: just re-add the connection.
                                            // Tools will be re-fetched from cache or fresh.
                                            let tools = {
                                                let cache = tool_cache.read().await;
                                                cache.get(name.as_str()).map(|c| c.tools.clone())
                                            }
                                            .unwrap_or_default();

                                            let mut conns = connections.write().await;
                                            conns.insert(
                                                name.clone(),
                                                ServerConnection {
                                                    transport: t,
                                                    tools,
                                                    config: prepared,
                                                },
                                            );
                                            true
                                        } else {
                                            false
                                        }
                                    }
                                    Err(_) => false,
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    };

                    let mut states = health_states.write().await;
                    if let Some(state) = states.get_mut(name.as_str()) {
                        if restart_ok {
                            state.status = ServerHealthStatus::Healthy;
                            state.consecutive_failures = 0;
                            state.tools_removed = false;
                            info!(server = %name, "Background restart succeeded");
                        } else if state.restart_attempts >= MAX_RESTART_ATTEMPTS {
                            state.status = ServerHealthStatus::PermanentlyFailed;
                            error!(
                                server = %name,
                                "Server permanently failed after {} restart attempts",
                                MAX_RESTART_ATTEMPTS
                            );
                        } else {
                            warn!(server = %name, "Background restart failed");
                        }
                    }
                }
            }
        });

        let mut handle_guard = self.health_check_handle.write().await;
        // Abort any existing task.
        if let Some(old) = handle_guard.take() {
            old.abort();
        }
        *handle_guard = Some(handle);
        info!(
            interval_secs = self.health_check_interval_secs,
            "Started MCP health monitoring"
        );
    }

    /// Stop background health monitoring.
    pub async fn stop_health_monitoring(&self) {
        let mut handle_guard = self.health_check_handle.write().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
            debug!("Stopped MCP health monitoring");
        }
    }

    /// Handle a `tools/changed` notification from a server.
    ///
    /// Invalidates the tool cache and refreshes tools if the server
    /// is still connected.
    pub async fn handle_tools_changed(&self, server_name: &str) {
        info!(
            server = server_name,
            "Received tools/changed notification, refreshing tools"
        );
        self.invalidate_tool_cache(server_name).await;

        // Try to refresh tools if connected.
        match self.refresh_tools(server_name).await {
            Ok(tools) => {
                info!(
                    server = server_name,
                    tools = tools.len(),
                    "Tools refreshed after tools/changed notification"
                );
            }
            Err(e) => {
                warn!(
                    server = server_name,
                    error = %e,
                    "Failed to refresh tools after tools/changed notification"
                );
            }
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

    /// Get a prompt from a specific server with optional arguments.
    ///
    /// Sends `prompts/get` to the server with the prompt name and any arguments.
    /// Returns the prompt messages that can be injected into the conversation.
    pub async fn get_prompt(
        &self,
        server_name: &str,
        prompt_name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> McpResult<McpPromptResult> {
        let connections = self.connections.read().await;
        let conn = connections.get(server_name).ok_or_else(|| {
            McpError::ServerNotFound(server_name.to_string())
        })?;

        let mut params = HashMap::new();
        params.insert(
            "name".to_string(),
            serde_json::Value::String(prompt_name.to_string()),
        );
        if let Some(args) = arguments {
            params.insert("arguments".to_string(), serde_json::to_value(args).unwrap());
        }

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_request_id(),
            method: "prompts/get".to_string(),
            params: Some(params),
        };

        let response = conn.transport.send_request(&request).await?;

        if let Some(error) = response.error {
            return Err(McpError::Protocol(format!(
                "prompts/get failed: {}",
                error.message
            )));
        }

        let result = response.result.ok_or_else(|| {
            McpError::Protocol("prompts/get returned no result".to_string())
        })?;

        serde_json::from_value(result)
            .map_err(|e| McpError::Protocol(format!("Failed to parse prompt result: {e}")))
    }

    /// List resources from all connected servers.
    ///
    /// Sends `resources/list` to each connected server and aggregates the results.
    pub async fn list_resources(&self) -> Vec<(String, McpResource)> {
        let connections = self.connections.read().await;
        let mut resources = Vec::new();

        for (server_name, conn) in connections.iter() {
            let request = JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: self.next_request_id(),
                method: "resources/list".to_string(),
                params: None,
            };

            match conn.transport.send_request(&request).await {
                Ok(response) => {
                    if let Some(result) = response.result
                        && let Some(resource_list) =
                            result.get("resources").and_then(|r| r.as_array())
                    {
                        for res_val in resource_list {
                            if let Ok(resource) =
                                serde_json::from_value::<McpResource>(res_val.clone())
                            {
                                resources.push((server_name.clone(), resource));
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to list resources from '{}': {}", server_name, e);
                }
            }
        }

        resources
    }

    /// Read a specific resource from a server.
    ///
    /// Sends `resources/read` with the resource URI and returns the content.
    pub async fn read_resource(
        &self,
        server_name: &str,
        resource_uri: &str,
    ) -> McpResult<Vec<McpContent>> {
        let connections = self.connections.read().await;
        let conn = connections.get(server_name).ok_or_else(|| {
            McpError::ServerNotFound(server_name.to_string())
        })?;

        let mut params = HashMap::new();
        params.insert(
            "uri".to_string(),
            serde_json::Value::String(resource_uri.to_string()),
        );

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_request_id(),
            method: "resources/read".to_string(),
            params: Some(params),
        };

        let response = conn.transport.send_request(&request).await?;

        if let Some(error) = response.error {
            return Err(McpError::Protocol(format!(
                "resources/read failed: {}",
                error.message
            )));
        }

        let result = response.result.ok_or_else(|| {
            McpError::Protocol("resources/read returned no result".to_string())
        })?;

        // Parse content array from response
        let contents = result
            .get("contents")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v: &serde_json::Value| {
                        // MCP resources return {uri, text?, blob?, mimeType?}
                        let text = v.get("text").and_then(|t| t.as_str());
                        if let Some(text) = text {
                            Some(McpContent::Text {
                                text: text.to_string(),
                            })
                        } else {
                            let blob = v.get("blob").and_then(|b| b.as_str());
                            let mime = v
                                .get("mimeType")
                                .and_then(|m| m.as_str())
                                .unwrap_or("application/octet-stream");
                            blob.map(|data: &str| McpContent::Image {
                                data: data.to_string(),
                                mime_type: mime.to_string(),
                            })
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(contents)
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

        // Clean up health state and cache.
        {
            let mut health_states = self.health_states.write().await;
            health_states.remove(name);
        }
        {
            let mut cache = self.tool_schema_cache.write().await;
            cache.remove(name);
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
    elif method == "ping":
        write_message({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {}
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

        // Test health check ping.
        let ping_ok = manager.ping_server("mock").await;
        assert!(ping_ok);

        // Verify health state.
        let health = manager.get_health_state("mock").await.unwrap();
        assert_eq!(health.status, ServerHealthStatus::Healthy);
        assert_eq!(health.consecutive_failures, 0);

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

    // -----------------------------------------------------------------------
    // Health monitoring tests (#75)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_health_state_default() {
        let state = ServerHealthState::default();
        assert_eq!(state.status, ServerHealthStatus::Healthy);
        assert_eq!(state.consecutive_failures, 0);
        assert_eq!(state.restart_attempts, 0);
        assert!(!state.tools_removed);
    }

    #[tokio::test]
    async fn test_health_check_success_resets_failures() {
        let manager = McpManager::new(None);

        // Initialize health state with some failures.
        {
            let mut states = manager.health_states.write().await;
            states.insert(
                "test-server".to_string(),
                ServerHealthState {
                    status: ServerHealthStatus::Degraded,
                    consecutive_failures: 2,
                    restart_attempts: 0,
                    tools_removed: false,
                },
            );
        }

        manager.record_health_check("test-server", true).await;

        let state = manager.get_health_state("test-server").await.unwrap();
        assert_eq!(state.status, ServerHealthStatus::Healthy);
        assert_eq!(state.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_health_check_failures_degrade_then_unhealthy() {
        let manager = McpManager::new(None);

        // Initialize healthy state.
        {
            let mut states = manager.health_states.write().await;
            states.insert("test-server".to_string(), ServerHealthState::default());
        }

        // Record failures up to threshold - 1.
        for _ in 0..HEALTH_CHECK_FAILURE_THRESHOLD - 1 {
            manager.record_health_check("test-server", false).await;
        }

        let state = manager.get_health_state("test-server").await.unwrap();
        assert_eq!(state.status, ServerHealthStatus::Degraded);
        assert_eq!(
            state.consecutive_failures,
            HEALTH_CHECK_FAILURE_THRESHOLD - 1
        );

        // One more failure pushes to unhealthy.
        manager.record_health_check("test-server", false).await;

        let state = manager.get_health_state("test-server").await.unwrap();
        assert_eq!(state.status, ServerHealthStatus::Unhealthy);
        assert!(state.tools_removed);
    }

    #[tokio::test]
    async fn test_permanently_failed_not_restarted() {
        let manager = McpManager::new(None);

        {
            let mut states = manager.health_states.write().await;
            states.insert(
                "dead-server".to_string(),
                ServerHealthState {
                    status: ServerHealthStatus::PermanentlyFailed,
                    consecutive_failures: 10,
                    restart_attempts: MAX_RESTART_ATTEMPTS,
                    tools_removed: true,
                },
            );
        }

        // record_health_check should be a no-op for permanently failed.
        manager.record_health_check("dead-server", false).await;
        let state = manager.get_health_state("dead-server").await.unwrap();
        assert_eq!(state.status, ServerHealthStatus::PermanentlyFailed);

        // attempt_restart should return false.
        let restarted = manager.attempt_restart("dead-server").await;
        assert!(!restarted);
    }

    // -----------------------------------------------------------------------
    // Exponential backoff tests (#77)
    // -----------------------------------------------------------------------

    #[test]
    fn test_backoff_calculation() {
        let mut state = ServerHealthState::default();
        assert_eq!(state.next_backoff_secs(), 1); // 2^0 = 1

        state.restart_attempts = 1;
        assert_eq!(state.next_backoff_secs(), 2); // 2^1 = 2

        state.restart_attempts = 2;
        assert_eq!(state.next_backoff_secs(), 4); // 2^2 = 4

        state.restart_attempts = 3;
        assert_eq!(state.next_backoff_secs(), 8); // 2^3 = 8

        // Should cap at MAX_BACKOFF_SECS.
        state.restart_attempts = 10;
        assert_eq!(state.next_backoff_secs(), MAX_BACKOFF_SECS);
    }

    #[tokio::test]
    async fn test_restart_increments_attempts() {
        let manager = McpManager::new(Some(PathBuf::from("/tmp")));

        // Set up empty config so restart will fail (no server config).
        {
            let mut config = manager.config.write().await;
            *config = Some(McpConfig::default());
        }

        {
            let mut states = manager.health_states.write().await;
            states.insert(
                "restart-test".to_string(),
                ServerHealthState {
                    status: ServerHealthStatus::Unhealthy,
                    consecutive_failures: 3,
                    restart_attempts: 0,
                    tools_removed: true,
                },
            );
        }

        // Restart will fail since the server doesn't exist in config.
        let result = manager.attempt_restart("restart-test").await;
        assert!(!result);

        let state = manager.get_health_state("restart-test").await.unwrap();
        assert_eq!(state.restart_attempts, 1);
    }

    #[tokio::test]
    async fn test_max_restart_attempts_marks_permanently_failed() {
        let manager = McpManager::new(Some(PathBuf::from("/tmp")));

        {
            let mut config = manager.config.write().await;
            *config = Some(McpConfig::default());
        }

        {
            let mut states = manager.health_states.write().await;
            states.insert(
                "doomed".to_string(),
                ServerHealthState {
                    status: ServerHealthStatus::Unhealthy,
                    consecutive_failures: 5,
                    restart_attempts: MAX_RESTART_ATTEMPTS - 1,
                    tools_removed: true,
                },
            );
        }

        // This should be the last attempt.
        let result = manager.attempt_restart("doomed").await;
        assert!(!result);

        let state = manager.get_health_state("doomed").await.unwrap();
        assert_eq!(state.status, ServerHealthStatus::PermanentlyFailed);
    }

    // -----------------------------------------------------------------------
    // Tool schema caching tests (#79)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_tool_cache_invalidation() {
        let manager = McpManager::new(None);

        // Populate cache directly.
        {
            let mut cache = manager.tool_schema_cache.write().await;
            cache.insert(
                "test-server".to_string(),
                ToolSchemaCache {
                    tools: vec![McpTool {
                        name: "cached_tool".to_string(),
                        description: "A cached tool".to_string(),
                        input_schema: serde_json::json!({}),
                    }],
                    invalidated: false,
                },
            );
        }

        // Verify cache is valid.
        {
            let cache = manager.tool_schema_cache.read().await;
            let entry = cache.get("test-server").unwrap();
            assert!(!entry.invalidated);
            assert_eq!(entry.tools.len(), 1);
        }

        // Invalidate.
        manager.invalidate_tool_cache("test-server").await;

        {
            let cache = manager.tool_schema_cache.read().await;
            let entry = cache.get("test-server").unwrap();
            assert!(entry.invalidated);
        }
    }

    #[tokio::test]
    async fn test_invalidate_all_tool_caches() {
        let manager = McpManager::new(None);

        {
            let mut cache = manager.tool_schema_cache.write().await;
            cache.insert(
                "server-a".to_string(),
                ToolSchemaCache {
                    tools: vec![],
                    invalidated: false,
                },
            );
            cache.insert(
                "server-b".to_string(),
                ToolSchemaCache {
                    tools: vec![],
                    invalidated: false,
                },
            );
        }

        manager.invalidate_all_tool_caches().await;

        let cache = manager.tool_schema_cache.read().await;
        assert!(cache.get("server-a").unwrap().invalidated);
        assert!(cache.get("server-b").unwrap().invalidated);
    }

    // -----------------------------------------------------------------------
    // Graceful failure tests (#58)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_ping_nonexistent_server_returns_false() {
        let manager = McpManager::new(None);
        assert!(!manager.ping_server("nonexistent").await);
    }

    #[tokio::test]
    async fn test_get_all_health_states() {
        let manager = McpManager::new(None);

        {
            let mut states = manager.health_states.write().await;
            states.insert("s1".to_string(), ServerHealthState::default());
            states.insert(
                "s2".to_string(),
                ServerHealthState {
                    status: ServerHealthStatus::Unhealthy,
                    consecutive_failures: 3,
                    restart_attempts: 1,
                    tools_removed: true,
                },
            );
        }

        let all = manager.get_all_health_states().await;
        assert_eq!(all.len(), 2);
        assert_eq!(all["s1"].status, ServerHealthStatus::Healthy);
        assert_eq!(all["s2"].status, ServerHealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_remove_server_cleans_up_health_and_cache() {
        let manager = McpManager::new(Some(PathBuf::from("/tmp")));

        {
            let mut config = manager.config.write().await;
            let mut mcp_config = McpConfig::default();
            mcp_config.mcp_servers.insert(
                "cleanup-test".to_string(),
                McpServerConfig {
                    command: "node".to_string(),
                    args: vec!["test.js".to_string()],
                    ..Default::default()
                },
            );
            *config = Some(mcp_config);
        }

        // Add health state and cache.
        {
            let mut states = manager.health_states.write().await;
            states.insert("cleanup-test".to_string(), ServerHealthState::default());
        }
        {
            let mut cache = manager.tool_schema_cache.write().await;
            cache.insert(
                "cleanup-test".to_string(),
                ToolSchemaCache {
                    tools: vec![],
                    invalidated: false,
                },
            );
        }

        manager.remove_server("cleanup-test").await.unwrap();

        assert!(manager.get_health_state("cleanup-test").await.is_none());
        let cache = manager.tool_schema_cache.read().await;
        assert!(cache.get("cleanup-test").is_none());
    }

    #[tokio::test]
    async fn test_with_health_check_interval() {
        let manager = McpManager::new(None).with_health_check_interval(60);
        assert_eq!(manager.health_check_interval_secs, 60);
    }

    #[tokio::test]
    async fn test_health_monitoring_disabled_when_zero() {
        let manager = McpManager::new(None).with_health_check_interval(0);
        // Should not panic, just return.
        manager.start_health_monitoring().await;
        // Verify no handle was set.
        let handle = manager.health_check_handle.read().await;
        assert!(handle.is_none());
    }

    // -----------------------------------------------------------------------
    // OAuth token acquisition tests (#74)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_acquire_oauth_token_invalid_url() {
        let oauth = McpOAuthConfig {
            client_id: "test-client".to_string(),
            client_secret: "test-secret".to_string(),
            token_url: "http://127.0.0.1:1/nonexistent/token".to_string(),
            scope: Some("read write".to_string()),
        };
        let result = McpManager::acquire_oauth_token(&oauth).await;
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("OAuth token request failed"));
    }

    #[test]
    fn test_oauth_config_struct() {
        let oauth = McpOAuthConfig {
            client_id: "cid".to_string(),
            client_secret: "csecret".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scope: Some("mcp:tools".to_string()),
        };
        assert_eq!(oauth.client_id, "cid");
        assert_eq!(oauth.scope.as_deref(), Some("mcp:tools"));

        // Test equality
        let oauth2 = oauth.clone();
        assert_eq!(oauth, oauth2);
    }

    #[test]
    fn test_oauth_config_serialization() {
        let oauth = McpOAuthConfig {
            client_id: "cid".to_string(),
            client_secret: "csecret".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scope: None,
        };
        let json = serde_json::to_string(&oauth).unwrap();
        let parsed: McpOAuthConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.client_id, "cid");
        assert!(parsed.scope.is_none());
    }

    // --- stop_health_monitoring tests ---

    #[tokio::test]
    async fn test_stop_health_monitoring_when_not_running() {
        let manager = McpManager::new(None);
        // Should be a no-op, not panic
        manager.stop_health_monitoring().await;
        let handle = manager.health_check_handle.read().await;
        assert!(handle.is_none());
    }

    #[tokio::test]
    async fn test_stop_health_monitoring_clears_handle() {
        let manager = McpManager::new(None);
        // Manually set a dummy handle
        {
            let mut handle = manager.health_check_handle.write().await;
            *handle = Some(tokio::spawn(async {
                // Simulate long-running task
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }));
        }
        assert!(manager.health_check_handle.read().await.is_some());

        manager.stop_health_monitoring().await;
        assert!(manager.health_check_handle.read().await.is_none());
    }

    // --- connect_all tests ---

    #[tokio::test]
    async fn test_connect_all_skips_disabled_servers() {
        let manager = McpManager::new(Some(PathBuf::from("/tmp")));
        {
            let mut config = manager.config.write().await;
            let mut cfg = McpConfig::default();
            cfg.mcp_servers.insert(
                "disabled_server".to_string(),
                McpServerConfig {
                    enabled: false,
                    auto_start: true,
                    ..McpServerConfig::default()
                },
            );
            cfg.mcp_servers.insert(
                "non_autostart".to_string(),
                McpServerConfig {
                    enabled: true,
                    auto_start: false,
                    ..McpServerConfig::default()
                },
            );
            *config = Some(cfg);
        }

        // Both servers should be skipped (no connection attempts)
        let connected = manager.connect_all().await.unwrap();
        assert!(connected.is_empty());
    }

    // --- disconnect_all tests ---

    #[tokio::test]
    async fn test_disconnect_all_empty() {
        let manager = McpManager::new(None);
        let result = manager.disconnect_all().await;
        assert!(result.is_ok());
        assert_eq!(manager.connected_count().await, 0);
    }

    // --- is_connected / connected_count explicit tests ---

    #[tokio::test]
    async fn test_is_connected_returns_false_for_unknown() {
        let manager = McpManager::new(None);
        assert!(!manager.is_connected("unknown").await);
    }

    #[tokio::test]
    async fn test_connected_count_starts_at_zero() {
        let manager = McpManager::new(None);
        assert_eq!(manager.connected_count().await, 0);
    }

    // --- list_prompts empty ---

    #[tokio::test]
    async fn test_list_prompts_no_connections() {
        let manager = McpManager::new(None);
        let prompts = manager.list_prompts().await;
        assert!(prompts.is_empty());
    }

    // --- handle_tools_changed on disconnected server ---

    #[tokio::test]
    async fn test_handle_tools_changed_nonexistent_server() {
        let manager = McpManager::new(None);
        // Should not panic, just log warning
        manager.handle_tools_changed("nonexistent").await;
    }

    // --- get_health_state ---

    #[tokio::test]
    async fn test_get_health_state_unknown_server() {
        let manager = McpManager::new(None);
        let state = manager.get_health_state("unknown").await;
        assert!(state.is_none());
    }

    // --- refresh_tools on disconnected server ---

    #[tokio::test]
    async fn test_refresh_tools_nonexistent_server() {
        let manager = McpManager::new(None);
        let result = manager.refresh_tools("nonexistent").await;
        assert!(result.is_err());
    }

    // --- Tool name sanitization ---

    #[test]
    fn test_sanitize_mcp_name_simple() {
        assert_eq!(sanitize_mcp_name("my-server"), "my-server");
        assert_eq!(sanitize_mcp_name("my_tool"), "my_tool");
        assert_eq!(sanitize_mcp_name("tool123"), "tool123");
    }

    #[test]
    fn test_sanitize_mcp_name_special_chars() {
        assert_eq!(sanitize_mcp_name("tool/name"), "tool_name");
        assert_eq!(sanitize_mcp_name("my.server"), "my_server");
        assert_eq!(sanitize_mcp_name("ns:tool"), "ns_tool");
        assert_eq!(sanitize_mcp_name("a b c"), "a_b_c");
    }

    #[test]
    fn test_sanitize_mcp_name_preserves_valid() {
        assert_eq!(sanitize_mcp_name("ABC-xyz_123"), "ABC-xyz_123");
        assert_eq!(sanitize_mcp_name(""), "");
    }

    // --- MCP get_prompt tests ---

    #[tokio::test]
    async fn test_get_prompt_disconnected_server() {
        let manager = McpManager::new(None);
        let result = manager.get_prompt("nonexistent", "test-prompt", None).await;
        assert!(matches!(result, Err(McpError::ServerNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_prompt_with_arguments_disconnected() {
        let manager = McpManager::new(None);
        let mut args = HashMap::new();
        args.insert("key".to_string(), "value".to_string());
        let result = manager
            .get_prompt("nonexistent", "test-prompt", Some(args))
            .await;
        assert!(matches!(result, Err(McpError::ServerNotFound(_))));
    }

    // --- MCP list_resources tests ---

    #[tokio::test]
    async fn test_list_resources_no_connections() {
        let manager = McpManager::new(None);
        let resources = manager.list_resources().await;
        assert!(resources.is_empty());
    }

    // --- MCP read_resource tests ---

    #[tokio::test]
    async fn test_read_resource_disconnected_server() {
        let manager = McpManager::new(None);
        let result = manager
            .read_resource("nonexistent", "file:///test.txt")
            .await;
        assert!(matches!(result, Err(McpError::ServerNotFound(_))));
    }
}
