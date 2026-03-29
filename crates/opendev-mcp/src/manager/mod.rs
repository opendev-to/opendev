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

mod connection;
mod health;
mod protocol;
mod resources;
mod tools;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::config::McpServerConfig;
use crate::models::McpTool;
use crate::transport::McpTransport;

pub use self::health::{ServerHealthState, ServerHealthStatus};

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
    config: Arc<RwLock<Option<crate::config::McpConfig>>>,
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

#[cfg(test)]
mod tests;
