//! Server connection lifecycle: connect, disconnect, list, add, remove.

use tracing::{debug, error, info, warn};

use crate::config::{McpConfig, McpServerConfig, prepare_server_config};
use crate::error::{McpError, McpResult};
use crate::models::McpServerInfo;
use crate::transport;

use super::{McpManager, ServerConnection, ServerHealthState};

impl McpManager {
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

    /// Connect to all enabled auto-start servers in parallel.
    ///
    /// All servers connect concurrently so the total wait time is
    /// bounded by the slowest server, not the sum of all timeouts.
    pub async fn connect_all(&self) -> McpResult<Vec<String>> {
        let config = self.get_config().await?;
        let servers: Vec<String> = config
            .mcp_servers
            .iter()
            .filter(|(_, sc)| sc.enabled && sc.auto_start)
            .map(|(name, _)| name.clone())
            .collect();

        let results =
            futures::future::join_all(servers.iter().map(|name| self.connect_server(name))).await;

        let mut connected = Vec::new();
        for (name, result) in servers.into_iter().zip(results) {
            match result {
                Ok(()) => connected.push(name),
                Err(e) => {
                    error!("Failed to connect to MCP server '{}': {}", name, e);
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

    /// Remove a failed server from the connection pool, closing its transport
    /// and dropping its tools from the registry.
    pub(super) async fn remove_failed_server(&self, name: &str) {
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
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
