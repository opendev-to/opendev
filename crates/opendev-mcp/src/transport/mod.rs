//! Transport abstraction for MCP server connections.
//!
//! Provides a trait for different transport mechanisms (stdio, SSE, HTTP)
//! and implementations for each.

mod framing;
mod http;
mod process;
mod sse;
mod stdio;

pub use http::HttpTransport;
pub use sse::SseTransport;
pub use stdio::StdioTransport;

use async_trait::async_trait;
use tokio::time::Duration;

use crate::config::{McpServerConfig, TransportType};
use crate::error::{McpError, McpResult};
use crate::models::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

/// Default timeout for a single request/response cycle.
pub(super) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Transport trait for communicating with MCP servers.
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Connect the transport (e.g., spawn child process).
    async fn connect(&mut self) -> McpResult<()>;

    /// Send a JSON-RPC request and receive a response.
    async fn send_request(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse>;

    /// Send a JSON-RPC notification (no response expected).
    async fn send_notification(&self, notification: &JsonRpcNotification) -> McpResult<()>;

    /// Close the transport connection.
    async fn close(&self) -> McpResult<()>;

    /// Check if the transport is currently connected.
    fn is_connected(&self) -> bool;

    /// Get the transport type name.
    fn transport_type(&self) -> &str;

    /// Take the notification receiver for server-initiated notifications.
    ///
    /// Returns `None` if the transport doesn't support notifications or
    /// the receiver has already been taken. The caller should spawn a task
    /// to consume incoming [`JsonRpcNotification`]s from the returned receiver.
    async fn take_notification_receiver(
        &mut self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<JsonRpcNotification>> {
        None
    }
}

/// Create the appropriate transport from a server configuration.
pub fn create_transport(config: &McpServerConfig) -> McpResult<Box<dyn McpTransport>> {
    match config.transport {
        TransportType::Http => {
            let url = config
                .url
                .as_ref()
                .ok_or_else(|| McpError::Config("HTTP transport requires a URL".to_string()))?;
            Ok(Box::new(HttpTransport::new(
                url.clone(),
                config.headers.clone(),
            )))
        }
        TransportType::Sse => {
            let url = config
                .url
                .as_ref()
                .ok_or_else(|| McpError::Config("SSE transport requires a URL".to_string()))?;
            Ok(Box::new(SseTransport::new(
                url.clone(),
                config.headers.clone(),
            )))
        }
        TransportType::Stdio => Ok(Box::new(create_stdio_transport(config)?)),
    }
}

/// Create a stdio transport based on the command type.
///
/// Maps command types (npx, node, python, uvx, etc.) to appropriate
/// transport configurations, mirroring the Python TransportMixin behavior.
fn create_stdio_transport(config: &McpServerConfig) -> McpResult<StdioTransport> {
    let command = &config.command;
    let args = &config.args;

    if command.is_empty() {
        return Err(McpError::Config(
            "Stdio transport requires a command".to_string(),
        ));
    }

    // Validate commands that require arguments
    match command.as_str() {
        "npx" | "node" | "python" | "python3" | "uvx" | "uv" if args.is_empty() => {
            return Err(McpError::Config(format!(
                "{command} command requires at least one argument"
            )));
        }
        _ => {}
    }

    Ok(StdioTransport::new(
        command.clone(),
        args.clone(),
        config.env.clone(),
    ))
}

#[cfg(test)]
mod tests;
