//! File I/O helpers for MCP server configuration persistence.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::WebError;

use super::models::{McpConfigFile, McpServerConfig};

/// Get the global MCP config path (~/.opendev/mcp.json).
pub(super) fn global_config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".opendev").join("mcp.json")
}

/// Get the project-level MCP config path (.opendev/mcp.json in working_dir).
pub(super) fn project_config_path(working_dir: &str) -> PathBuf {
    PathBuf::from(working_dir).join(".opendev").join("mcp.json")
}

/// Load MCP servers from both global and project config files.
pub(super) fn load_all_servers(working_dir: &str) -> HashMap<String, McpServerConfig> {
    let mut servers = HashMap::new();

    // Load global config.
    let global_path = global_config_path();
    if let Ok(content) = std::fs::read_to_string(&global_path)
        && let Ok(config) = serde_json::from_str::<McpConfigFile>(&content)
    {
        servers.extend(config.mcp_servers);
    }

    // Load project config (overrides global).
    let project_path = project_config_path(working_dir);
    if let Ok(content) = std::fs::read_to_string(&project_path)
        && let Ok(config) = serde_json::from_str::<McpConfigFile>(&content)
    {
        servers.extend(config.mcp_servers);
    }

    servers
}

/// Save a server config to the global MCP config file.
pub(super) fn save_server_to_config(
    name: &str,
    config: &McpServerConfig,
    config_path: &Path,
) -> Result<(), WebError> {
    // Ensure parent directory exists.
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| WebError::Internal(format!("Failed to create config directory: {}", e)))?;
    }

    // Read existing config.
    let mut mcp_config = if let Ok(content) = std::fs::read_to_string(config_path) {
        serde_json::from_str::<McpConfigFile>(&content).unwrap_or(McpConfigFile {
            mcp_servers: HashMap::new(),
        })
    } else {
        McpConfigFile {
            mcp_servers: HashMap::new(),
        }
    };

    mcp_config
        .mcp_servers
        .insert(name.to_string(), config.clone());

    let content = serde_json::to_string_pretty(&mcp_config)
        .map_err(|e| WebError::Internal(format!("Failed to serialize config: {}", e)))?;

    let tmp_path = config_path.with_extension("json.tmp");

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true).mode(0o600);

        let mut file = opts.open(&tmp_path).map_err(|e| {
            WebError::Internal(format!(
                "Failed to open temp config file {}: {}",
                tmp_path.display(),
                e
            ))
        })?;
        std::io::Write::write_all(&mut file, content.as_bytes()).map_err(|e| {
            WebError::Internal(format!(
                "Failed to write temp config file {}: {}",
                tmp_path.display(),
                e
            ))
        })?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(&tmp_path, content).map_err(|e| {
            WebError::Internal(format!(
                "Failed to write temp config file {}: {}",
                tmp_path.display(),
                e
            ))
        })?;
    }

    std::fs::rename(&tmp_path, config_path).map_err(|e| {
        WebError::Internal(format!(
            "Failed to rename temp config to {}: {}",
            config_path.display(),
            e
        ))
    })?;

    Ok(())
}

/// Remove a server from a config file.
pub(super) fn remove_server_from_config(name: &str, config_path: &Path) -> Result<bool, WebError> {
    if !config_path.exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(config_path)
        .map_err(|e| WebError::Internal(format!("Failed to read config: {}", e)))?;

    let mut mcp_config = serde_json::from_str::<McpConfigFile>(&content)
        .map_err(|e| WebError::Internal(format!("Failed to parse config: {}", e)))?;

    let removed = mcp_config.mcp_servers.remove(name).is_some();

    if removed {
        let content = serde_json::to_string_pretty(&mcp_config)
            .map_err(|e| WebError::Internal(format!("Failed to serialize config: {}", e)))?;

        let tmp_path = config_path.with_extension("json.tmp");

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true).mode(0o600);

            let mut file = opts.open(&tmp_path).map_err(|e| {
                WebError::Internal(format!(
                    "Failed to open temp config file {}: {}",
                    tmp_path.display(),
                    e
                ))
            })?;
            std::io::Write::write_all(&mut file, content.as_bytes()).map_err(|e| {
                WebError::Internal(format!(
                    "Failed to write temp config file {}: {}",
                    tmp_path.display(),
                    e
                ))
            })?;
        }

        #[cfg(not(unix))]
        {
            std::fs::write(&tmp_path, content).map_err(|e| {
                WebError::Internal(format!(
                    "Failed to write temp config file {}: {}",
                    tmp_path.display(),
                    e
                ))
            })?;
        }

        std::fs::rename(&tmp_path, config_path).map_err(|e| {
            WebError::Internal(format!(
                "Failed to rename temp config to {}: {}",
                config_path.display(),
                e
            ))
        })?;
    }

    Ok(removed)
}

#[cfg(test)]
#[path = "io_tests.rs"]
mod tests;
