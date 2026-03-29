//! Tool discovery, caching, invocation, and schema management.

use std::collections::HashMap;

use tracing::{debug, info, warn};

use crate::error::{McpError, McpResult};
use crate::models::{JsonRpcRequest, McpContent, McpTool, McpToolResult, McpToolSchema};
use crate::transport::McpTransport;

use super::{McpManager, NotificationHandle, ToolSchemaCache, sanitize_mcp_name};

impl NotificationHandle {
    /// Invalidate cache and re-discover tools for a server.
    pub(super) async fn handle_tools_changed(&self, server_name: &str) {
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
                    && let Ok(tools) = serde_json::from_value::<Vec<McpTool>>(tools_val.clone())
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
    /// Discover tools from a connected server via tools/list.
    pub(super) async fn discover_tools(
        &self,
        transport: &dyn McpTransport,
    ) -> McpResult<Vec<McpTool>> {
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

    /// Get tools from cache or discover them fresh.
    pub(super) async fn get_or_discover_tools(
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
                warn!(
                    server = server_name,
                    tool = tool_name,
                    error = %result.as_ref().unwrap_err(),
                    "MCP server failed, removing from active connections"
                );
                self.remove_failed_server(server_name).await;

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
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
