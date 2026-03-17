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
mod tests {
    use super::*;
    use crate::config::McpConfig;
    use crate::manager::MAX_RESTART_ATTEMPTS;
    use std::path::PathBuf;

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
}
