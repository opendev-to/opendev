//! Health monitoring, ping checks, and auto-restart with exponential backoff.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::{debug, error, info, warn};

use crate::config::prepare_server_config;
use crate::models::JsonRpcRequest;
use crate::transport;

use super::{
    HEALTH_CHECK_FAILURE_THRESHOLD, MAX_BACKOFF_SECS, MAX_RESTART_ATTEMPTS, McpManager,
    ServerConnection,
};

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

impl McpManager {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::McpConfig;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_health_state_default() {
        let state = ServerHealthState::default();
        assert_eq!(state.status, ServerHealthStatus::Healthy);
        assert_eq!(state.consecutive_failures, 0);
        assert_eq!(state.restart_attempts, 0);
        assert!(!state.tools_removed);
    }

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
    async fn test_health_check_success_resets_failures() {
        let manager = McpManager::new(None);

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

        {
            let mut states = manager.health_states.write().await;
            states.insert("test-server".to_string(), ServerHealthState::default());
        }

        for _ in 0..HEALTH_CHECK_FAILURE_THRESHOLD - 1 {
            manager.record_health_check("test-server", false).await;
        }

        let state = manager.get_health_state("test-server").await.unwrap();
        assert_eq!(state.status, ServerHealthStatus::Degraded);
        assert_eq!(
            state.consecutive_failures,
            HEALTH_CHECK_FAILURE_THRESHOLD - 1
        );

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

        manager.record_health_check("dead-server", false).await;
        let state = manager.get_health_state("dead-server").await.unwrap();
        assert_eq!(state.status, ServerHealthStatus::PermanentlyFailed);

        let restarted = manager.attempt_restart("dead-server").await;
        assert!(!restarted);
    }

    #[tokio::test]
    async fn test_restart_increments_attempts() {
        let manager = McpManager::new(Some(PathBuf::from("/tmp")));

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

        let result = manager.attempt_restart("doomed").await;
        assert!(!result);

        let state = manager.get_health_state("doomed").await.unwrap();
        assert_eq!(state.status, ServerHealthStatus::PermanentlyFailed);
    }

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
    async fn test_get_health_state_unknown_server() {
        let manager = McpManager::new(None);
        let state = manager.get_health_state("unknown").await;
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn test_health_monitoring_disabled_when_zero() {
        let manager = McpManager::new(None).with_health_check_interval(0);
        manager.start_health_monitoring().await;
        let handle = manager.health_check_handle.read().await;
        assert!(handle.is_none());
    }

    #[tokio::test]
    async fn test_stop_health_monitoring_when_not_running() {
        let manager = McpManager::new(None);
        manager.stop_health_monitoring().await;
        let handle = manager.health_check_handle.read().await;
        assert!(handle.is_none());
    }

    #[tokio::test]
    async fn test_stop_health_monitoring_clears_handle() {
        let manager = McpManager::new(None);
        {
            let mut handle = manager.health_check_handle.write().await;
            *handle = Some(tokio::spawn(async {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }));
        }
        assert!(manager.health_check_handle.read().await.is_some());

        manager.stop_health_monitoring().await;
        assert!(manager.health_check_handle.read().await.is_none());
    }
}
