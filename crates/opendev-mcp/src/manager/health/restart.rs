//! Restart orchestration with exponential backoff for unhealthy MCP servers.

use tracing::{debug, error, info, warn};

use super::ServerHealthStatus;
use crate::manager::{MAX_RESTART_ATTEMPTS, McpManager};

impl McpManager {
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
}

#[cfg(test)]
#[path = "restart_tests.rs"]
mod tests;
