//! MCP server configuration management.
//!
//! Handles loading, saving, and merging MCP server configurations
//! from global (~/.opendev/mcp.json) and project-level (.mcp.json) files.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{McpError, McpResult};

/// Transport type for MCP server connections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum TransportType {
    #[default]
    Stdio,
    Sse,
    Http,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdio => write!(f, "stdio"),
            Self::Sse => write!(f, "sse"),
            Self::Http => write!(f, "http"),
        }
    }
}

/// OAuth 2.0 configuration for MCP server authentication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpOAuthConfig {
    /// OAuth client ID.
    pub client_id: String,
    /// OAuth client secret.
    pub client_secret: String,
    /// Token endpoint URL.
    pub token_url: String,
    /// OAuth scope (space-separated).
    #[serde(default)]
    pub scope: Option<String>,
}

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Command to start the MCP server (for stdio transport).
    #[serde(default)]
    pub command: String,

    /// Arguments for the command.
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables to set.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// URL for HTTP/SSE transport.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// HTTP headers for remote servers.
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Whether the server is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Auto-start when OpenDev launches.
    #[serde(default = "default_true")]
    pub auto_start: bool,

    /// Transport type.
    #[serde(default)]
    pub transport: TransportType,

    /// Optional OAuth 2.0 configuration for server authentication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<McpOAuthConfig>,

    /// Per-server request timeout in milliseconds.
    /// If None, uses the default timeout (30 seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// Default MCP request timeout in milliseconds.
pub const DEFAULT_MCP_TIMEOUT_MS: u64 = 30_000;

fn default_true() -> bool {
    true
}

impl McpServerConfig {
    /// Get the effective timeout for this server.
    pub fn effective_timeout_ms(&self) -> u64 {
        self.timeout.unwrap_or(DEFAULT_MCP_TIMEOUT_MS)
    }
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            url: None,
            headers: HashMap::new(),
            enabled: true,
            auto_start: true,
            transport: TransportType::Stdio,
            oauth: None,
            timeout: None,
        }
    }
}

/// Root MCP configuration containing all server definitions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    /// Server configurations keyed by name.
    #[serde(alias = "mcpServers", default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

/// Load MCP configuration from a JSON file.
pub fn load_config(config_path: &Path) -> McpResult<McpConfig> {
    if !config_path.exists() {
        return Ok(McpConfig::default());
    }

    let content = std::fs::read_to_string(config_path).map_err(|e| {
        McpError::Config(format!(
            "Failed to read MCP config from {}: {}",
            config_path.display(),
            e
        ))
    })?;

    serde_json::from_str(&content).map_err(|e| {
        McpError::Config(format!(
            "Failed to parse MCP config from {}: {}",
            config_path.display(),
            e
        ))
    })
}

/// Save MCP configuration to a JSON file.
pub fn save_config(config: &McpConfig, config_path: &Path) -> McpResult<()> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            McpError::Config(format!(
                "Failed to create config directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(config_path, content).map_err(|e| {
        McpError::Config(format!(
            "Failed to write MCP config to {}: {}",
            config_path.display(),
            e
        ))
    })?;

    Ok(())
}

/// Merge global and project-level MCP configurations.
///
/// Project config takes precedence over global config.
pub fn merge_configs(global: &McpConfig, project: Option<&McpConfig>) -> McpConfig {
    let Some(project) = project else {
        return global.clone();
    };

    let mut merged = global.mcp_servers.clone();
    merged.extend(project.mcp_servers.clone());

    McpConfig {
        mcp_servers: merged,
    }
}

/// Expand environment variables in a string.
///
/// Supports `${VAR_NAME}` syntax. Variables not found in the environment
/// are left as-is.
pub fn expand_env_vars(value: &str) -> String {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    re.replace_all(value, |caps: &regex::Captures| {
        let var_name = &caps[1];
        std::env::var(var_name).unwrap_or_else(|_| caps[0].to_string())
    })
    .into_owned()
}

/// Prepare a server config by expanding environment variables in all fields.
pub fn prepare_server_config(config: &McpServerConfig) -> McpServerConfig {
    McpServerConfig {
        command: config.command.clone(),
        args: config.args.iter().map(|a| expand_env_vars(a)).collect(),
        env: config
            .env
            .iter()
            .map(|(k, v)| (k.clone(), expand_env_vars(v)))
            .collect(),
        url: config.url.as_ref().map(|u| expand_env_vars(u)),
        headers: config
            .headers
            .iter()
            .map(|(k, v)| (k.clone(), expand_env_vars(v)))
            .collect(),
        enabled: config.enabled,
        auto_start: config.auto_start,
        transport: config.transport.clone(),
        oauth: config.oauth.as_ref().map(|o| McpOAuthConfig {
            client_id: expand_env_vars(&o.client_id),
            client_secret: expand_env_vars(&o.client_secret),
            token_url: expand_env_vars(&o.token_url),
            scope: o.scope.as_ref().map(|s| expand_env_vars(s)),
        }),
        timeout: config.timeout,
    }
}

/// Get the project-level MCP config path if it exists.
pub fn get_project_config_path(working_dir: &Path) -> Option<PathBuf> {
    let config_path = working_dir.join(".mcp.json");
    if config_path.exists() {
        Some(config_path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_server_config() {
        let config = McpServerConfig::default();
        assert!(config.enabled);
        assert!(config.auto_start);
        assert_eq!(config.transport, TransportType::Stdio);
        assert!(config.command.is_empty());
    }

    #[test]
    fn test_config_roundtrip() {
        let mut config = McpConfig::default();
        config.mcp_servers.insert(
            "test-server".to_string(),
            McpServerConfig {
                command: "npx".to_string(),
                args: vec!["mcp-server-test".to_string()],
                ..Default::default()
            },
        );

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: McpConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.mcp_servers.contains_key("test-server"));
        assert_eq!(deserialized.mcp_servers["test-server"].command, "npx");
    }

    #[test]
    fn test_config_alias_deserialization() {
        let json = r#"{"mcpServers": {"my-server": {"command": "node", "args": ["server.js"]}}}"#;
        let config: McpConfig = serde_json::from_str(json).unwrap();
        assert!(config.mcp_servers.contains_key("my-server"));
        assert_eq!(config.mcp_servers["my-server"].command, "node");
    }

    #[test]
    fn test_merge_configs() {
        let mut global = McpConfig::default();
        global.mcp_servers.insert(
            "global-server".to_string(),
            McpServerConfig {
                command: "node".to_string(),
                ..Default::default()
            },
        );
        global.mcp_servers.insert(
            "shared".to_string(),
            McpServerConfig {
                command: "old".to_string(),
                ..Default::default()
            },
        );

        let mut project = McpConfig::default();
        project.mcp_servers.insert(
            "project-server".to_string(),
            McpServerConfig {
                command: "python".to_string(),
                ..Default::default()
            },
        );
        project.mcp_servers.insert(
            "shared".to_string(),
            McpServerConfig {
                command: "new".to_string(),
                ..Default::default()
            },
        );

        let merged = merge_configs(&global, Some(&project));
        assert_eq!(merged.mcp_servers.len(), 3);
        assert!(merged.mcp_servers.contains_key("global-server"));
        assert!(merged.mcp_servers.contains_key("project-server"));
        // Project overrides global
        assert_eq!(merged.mcp_servers["shared"].command, "new");
    }

    #[test]
    fn test_expand_env_vars() {
        // SAFETY: test-only; tests in this module are not run concurrently.
        unsafe { std::env::set_var("TEST_MCP_VAR", "hello") };
        assert_eq!(expand_env_vars("${TEST_MCP_VAR}_world"), "hello_world");
        // Unknown variables are left as-is
        assert_eq!(
            expand_env_vars("${UNKNOWN_VAR_12345}"),
            "${UNKNOWN_VAR_12345}"
        );
        // No variables
        assert_eq!(expand_env_vars("no vars here"), "no vars here");
        // SAFETY: test-only cleanup.
        unsafe { std::env::remove_var("TEST_MCP_VAR") };
    }

    #[test]
    fn test_prepare_server_config() {
        // SAFETY: test-only; not run concurrently with env-dependent code.
        unsafe { std::env::set_var("MCP_TEST_TOKEN", "secret123") };
        let config = McpServerConfig {
            command: "node".to_string(),
            args: vec![
                "server.js".to_string(),
                "--token=${MCP_TEST_TOKEN}".to_string(),
            ],
            headers: HashMap::from([(
                "Authorization".to_string(),
                "Bearer ${MCP_TEST_TOKEN}".to_string(),
            )]),
            url: Some("https://example.com/${MCP_TEST_TOKEN}".to_string()),
            ..Default::default()
        };

        let prepared = prepare_server_config(&config);
        assert_eq!(prepared.args[1], "--token=secret123");
        assert_eq!(prepared.headers["Authorization"], "Bearer secret123");
        assert_eq!(
            prepared.url.as_deref(),
            Some("https://example.com/secret123")
        );
        // SAFETY: test-only cleanup.
        unsafe { std::env::remove_var("MCP_TEST_TOKEN") };
    }

    #[test]
    fn test_transport_type_display() {
        assert_eq!(TransportType::Stdio.to_string(), "stdio");
        assert_eq!(TransportType::Sse.to_string(), "sse");
        assert_eq!(TransportType::Http.to_string(), "http");
    }

    #[test]
    fn test_oauth_config_deserialization() {
        let json = r#"{
            "mcpServers": {
                "auth-server": {
                    "command": "node",
                    "args": ["server.js"],
                    "transport": "http",
                    "url": "https://mcp.example.com",
                    "oauth": {
                        "client_id": "my-client",
                        "client_secret": "my-secret",
                        "token_url": "https://auth.example.com/token",
                        "scope": "mcp:read mcp:write"
                    }
                }
            }
        }"#;
        let config: McpConfig = serde_json::from_str(json).unwrap();
        let server = &config.mcp_servers["auth-server"];
        let oauth = server.oauth.as_ref().unwrap();
        assert_eq!(oauth.client_id, "my-client");
        assert_eq!(oauth.client_secret, "my-secret");
        assert_eq!(oauth.token_url, "https://auth.example.com/token");
        assert_eq!(oauth.scope.as_deref(), Some("mcp:read mcp:write"));
    }

    #[test]
    fn test_oauth_config_none_by_default() {
        let config = McpServerConfig::default();
        assert!(config.oauth.is_none());
    }

    #[test]
    fn test_prepare_expands_oauth_env_vars() {
        // SAFETY: test-only; not run concurrently with env-dependent code.
        unsafe { std::env::set_var("MCP_OAUTH_SECRET", "expanded_secret") };
        let config = McpServerConfig {
            oauth: Some(McpOAuthConfig {
                client_id: "client".to_string(),
                client_secret: "${MCP_OAUTH_SECRET}".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scope: Some("read".to_string()),
            }),
            ..Default::default()
        };
        let prepared = prepare_server_config(&config);
        let oauth = prepared.oauth.unwrap();
        assert_eq!(oauth.client_secret, "expanded_secret");
        // SAFETY: test-only cleanup.
        unsafe { std::env::remove_var("MCP_OAUTH_SECRET") };
    }

    #[test]
    fn test_load_config_missing_file() {
        let result = load_config(Path::new("/nonexistent/path/mcp.json"));
        assert!(result.is_ok());
        assert!(result.unwrap().mcp_servers.is_empty());
    }

    #[test]
    fn test_save_and_load_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("mcp.json");

        let mut config = McpConfig::default();
        config.mcp_servers.insert(
            "test".to_string(),
            McpServerConfig {
                command: "npx".to_string(),
                args: vec!["mcp-test".to_string()],
                ..Default::default()
            },
        );

        save_config(&config, &config_path).unwrap();
        let loaded = load_config(&config_path).unwrap();
        assert_eq!(loaded.mcp_servers.len(), 1);
        assert_eq!(loaded.mcp_servers["test"].command, "npx");
    }

    #[test]
    fn test_timeout_default() {
        let config = McpServerConfig::default();
        assert!(config.timeout.is_none());
        assert_eq!(config.effective_timeout_ms(), DEFAULT_MCP_TIMEOUT_MS);
    }

    #[test]
    fn test_timeout_custom() {
        let config = McpServerConfig {
            timeout: Some(60_000),
            ..Default::default()
        };
        assert_eq!(config.effective_timeout_ms(), 60_000);
    }

    #[test]
    fn test_timeout_deserialization() {
        let json = r#"{"mcpServers": {"slow-server": {"command": "node", "args": ["server.js"], "timeout": 120000}}}"#;
        let config: McpConfig = serde_json::from_str(json).unwrap();
        let server = &config.mcp_servers["slow-server"];
        assert_eq!(server.timeout, Some(120_000));
        assert_eq!(server.effective_timeout_ms(), 120_000);
    }

    #[test]
    fn test_timeout_not_serialized_when_none() {
        let config = McpServerConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("timeout"));
    }
}
