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

    let tmp_path = config_path.with_extension("json.tmp");

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true).mode(0o600);

        let mut file = opts
            .open(&tmp_path)
            .map_err(|e| McpError::Config(format!("Failed to open tmp config file: {}", e)))?;
        std::io::Write::write_all(&mut file, content.as_bytes())
            .map_err(|e| McpError::Config(format!("Failed to write tmp config file: {}", e)))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&tmp_path, content)
            .map_err(|e| McpError::Config(format!("Failed to write tmp config file: {}", e)))?;
    }

    std::fs::rename(&tmp_path, config_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        McpError::Config(format!(
            "Failed to rename config file {}: {}",
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
#[path = "config_tests.rs"]
mod tests;
