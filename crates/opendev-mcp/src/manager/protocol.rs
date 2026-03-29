//! MCP protocol operations: configuration, handshake, OAuth.

use std::collections::HashMap;
use std::path::PathBuf;

use tracing::{debug, info};

use crate::config::{
    McpConfig, McpOAuthConfig, get_project_config_path, load_config, merge_configs,
};
use crate::error::{McpError, McpResult};
use crate::models::{JsonRpcNotification, JsonRpcRequest};
use crate::transport::McpTransport;

use super::{DEFAULT_HEALTH_CHECK_INTERVAL_SECS, McpManager, NotificationHandle};

impl McpManager {
    /// Create a new MCP manager.
    pub fn new(working_dir: Option<PathBuf>) -> Self {
        use std::sync::Arc;
        use tokio::sync::RwLock;

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

    pub(super) fn next_request_id(&self) -> u64 {
        self.request_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Create a lightweight handle for notification listener tasks.
    pub(super) fn clone_for_notifications(&self) -> NotificationHandle {
        use std::sync::Arc;
        NotificationHandle {
            connections: Arc::clone(&self.connections),
            tool_schema_cache: Arc::clone(&self.tool_schema_cache),
            request_id: Arc::clone(&self.request_id),
        }
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
    pub(super) async fn initialize_handshake(
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
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
