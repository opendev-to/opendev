//! Transport abstraction for MCP server connections.
//!
//! Provides a trait for different transport mechanisms (stdio, SSE, HTTP)
//! and implementations for each.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, oneshot};
use tokio::time::{Duration, timeout};
use tracing::{debug, trace, warn};

use crate::config::{McpServerConfig, TransportType};
use crate::error::{McpError, McpResult};
use crate::models::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

/// Default timeout for a single request/response cycle.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

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

/// Internal state for a running stdio child process.
struct StdioProcess {
    child: Child,
    /// Pending response waiters keyed by request id.
    pending: HashMap<u64, oneshot::Sender<JsonRpcResponse>>,
}

/// Stdio transport: communicates with a child process via stdin/stdout.
pub struct StdioTransport {
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    /// Shared state for the running process and pending requests.
    state: Arc<Mutex<Option<StdioProcess>>>,
    /// Handle for the background reader task.
    reader_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Channel for server-initiated notifications (e.g., tools/changed).
    notification_tx: tokio::sync::mpsc::UnboundedSender<JsonRpcNotification>,
    notification_rx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<JsonRpcNotification>>>>,
}

impl StdioTransport {
    pub fn new(command: String, args: Vec<String>, env: HashMap<String, String>) -> Self {
        let (notification_tx, notification_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            command,
            args,
            env,
            state: Arc::new(Mutex::new(None)),
            reader_handle: Arc::new(Mutex::new(None)),
            notification_tx,
            notification_rx: Arc::new(Mutex::new(Some(notification_rx))),
        }
    }

    /// Take the notification receiver. Can only be called once; subsequent
    /// calls return `None`. The caller should spawn a task to consume
    /// incoming [`JsonRpcNotification`]s from the returned receiver.
    pub async fn take_notification_receiver(
        &self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<JsonRpcNotification>> {
        self.notification_rx.lock().await.take()
    }

    /// Get the command that will be executed.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Get the arguments.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Write a JSON payload to stdin using Content-Length header framing.
    async fn write_message(
        stdin: &mut tokio::process::ChildStdin,
        payload: &[u8],
    ) -> McpResult<()> {
        let header = format!("Content-Length: {}\r\n\r\n", payload.len());
        stdin.write_all(header.as_bytes()).await?;
        stdin.write_all(payload).await?;
        stdin.flush().await?;
        Ok(())
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn connect(&mut self) -> McpResult<()> {
        let mut lock = self.state.lock().await;
        if lock.is_some() {
            return Err(McpError::Transport(
                "Transport already connected".to_string(),
            ));
        }

        debug!(
            command = %self.command,
            args = ?self.args,
            "Spawning MCP stdio server"
        );

        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .envs(&self.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            McpError::Transport(format!("Failed to spawn '{}': {}", self.command, e))
        })?;

        // Take stdout for the reader task.
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Transport("Failed to capture child stdout".to_string()))?;

        // Take stderr to log it.
        let stderr = child.stderr.take();

        let process = StdioProcess {
            child,
            pending: HashMap::new(),
        };
        *lock = Some(process);
        drop(lock);

        // Spawn background stderr logger.
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    trace!(target: "mcp_stderr", "{}", line);
                }
            });
        }

        // Spawn background reader that parses Content-Length framed JSON-RPC
        // responses from stdout and dispatches them to pending waiters.
        // Server-initiated notifications are forwarded through notification_tx.
        let state = Arc::clone(&self.state);
        let notif_tx = self.notification_tx.clone();
        let handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader).await {
                    Ok(bytes) => {
                        match serde_json::from_slice::<JsonRpcResponse>(&bytes) {
                            Ok(response) => {
                                if let Some(id) = response.id {
                                    let mut lock = state.lock().await;
                                    if let Some(proc) = lock.as_mut() {
                                        if let Some(tx) = proc.pending.remove(&id) {
                                            let _ = tx.send(response);
                                        } else {
                                            warn!("Received response for unknown id: {}", id);
                                        }
                                    }
                                } else {
                                    // Server-initiated notification (no id).
                                    // Re-parse as notification to extract method name.
                                    match serde_json::from_slice::<JsonRpcNotification>(&bytes) {
                                        Ok(notif) => {
                                            debug!(
                                                method = %notif.method,
                                                "Received server notification"
                                            );
                                            let _ = notif_tx.send(notif);
                                        }
                                        Err(e) => {
                                            debug!(
                                                "Received server notification (unparseable: {})",
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse JSON-RPC response: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Stdio reader stopped: {}", e);
                        break;
                    }
                }
            }
        });

        let mut rh = self.reader_handle.lock().await;
        *rh = Some(handle);

        Ok(())
    }

    async fn send_request(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        let rx = {
            let mut lock = self.state.lock().await;
            let proc = lock
                .as_mut()
                .ok_or_else(|| McpError::Transport("Transport not connected".to_string()))?;

            let payload = serde_json::to_vec(request)?;

            let (tx, rx) = oneshot::channel();
            proc.pending.insert(request.id, tx);

            let stdin = proc
                .child
                .stdin
                .as_mut()
                .ok_or_else(|| McpError::Transport("Child stdin not available".to_string()))?;

            Self::write_message(stdin, &payload)
                .await
                .inspect_err(|_e| {
                    // Remove pending entry on write failure.
                    proc.pending.remove(&request.id);
                })?;

            rx
        };

        // Wait for response with timeout.
        let response = timeout(REQUEST_TIMEOUT, rx)
            .await
            .map_err(|_| McpError::Timeout(REQUEST_TIMEOUT.as_secs()))?
            .map_err(|_| McpError::Transport("Response channel closed unexpectedly".to_string()))?;

        Ok(response)
    }

    async fn send_notification(&self, notification: &JsonRpcNotification) -> McpResult<()> {
        let mut lock = self.state.lock().await;
        let proc = lock
            .as_mut()
            .ok_or_else(|| McpError::Transport("Transport not connected".to_string()))?;

        let payload = serde_json::to_vec(notification)?;

        let stdin = proc
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| McpError::Transport("Child stdin not available".to_string()))?;

        Self::write_message(stdin, &payload).await
    }

    async fn close(&self) -> McpResult<()> {
        // Abort the reader task.
        {
            let mut rh = self.reader_handle.lock().await;
            if let Some(handle) = rh.take() {
                handle.abort();
            }
        }

        let mut lock = self.state.lock().await;
        if let Some(mut proc) = lock.take() {
            // Collect descendant PIDs before killing the main process,
            // so we can clean up grandchild processes (e.g. Chrome spawned
            // by chrome-devtools-mcp) that would otherwise be orphaned.
            let descendant_pids = if let Some(pid) = proc.child.id() {
                collect_descendant_pids(pid)
            } else {
                vec![]
            };

            // Drop stdin to signal EOF to the child.
            drop(proc.child.stdin.take());

            // Give the child a moment to exit gracefully, then kill.
            match timeout(Duration::from_secs(2), proc.child.wait()).await {
                Ok(Ok(status)) => {
                    debug!("MCP server exited with status: {}", status);
                }
                Ok(Err(e)) => {
                    warn!("Error waiting for MCP server exit: {}", e);
                }
                Err(_) => {
                    warn!("MCP server did not exit in time, killing");
                    let _ = proc.child.kill().await;
                }
            }

            // Kill any descendant processes that may still be running.
            kill_descendant_pids(&descendant_pids);

            // Cancel any pending requests.
            for (id, tx) in proc.pending.drain() {
                debug!("Cancelling pending request {}", id);
                drop(tx);
            }
        }
        Ok(())
    }

    fn is_connected(&self) -> bool {
        // We can't async-lock here, so use try_lock as best effort.
        self.state
            .try_lock()
            .map(|guard| guard.is_some())
            .unwrap_or(true) // If locked, assume connected (in use).
    }

    fn transport_type(&self) -> &str {
        "stdio"
    }

    async fn take_notification_receiver(
        &mut self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<JsonRpcNotification>> {
        self.notification_rx.lock().await.take()
    }
}

/// Recursively collect all descendant PIDs of a process using `pgrep -P`.
///
/// Uses BFS to traverse the process tree, collecting all child and grandchild
/// PIDs. This is needed to clean up processes spawned by MCP servers (e.g.,
/// Chrome spawned by chrome-devtools-mcp).
#[cfg(unix)]
fn collect_descendant_pids(root_pid: u32) -> Vec<u32> {
    let mut descendants = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root_pid);

    while let Some(pid) = queue.pop_front() {
        match std::process::Command::new("pgrep")
            .args(["-P", &pid.to_string()])
            .output()
        {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Ok(child_pid) = line.trim().parse::<u32>() {
                        descendants.push(child_pid);
                        queue.push_back(child_pid);
                    }
                }
            }
            _ => {}
        }
    }

    descendants
}

#[cfg(not(unix))]
fn collect_descendant_pids(_root_pid: u32) -> Vec<u32> {
    Vec::new()
}

/// Send SIGTERM to each descendant PID, logging any that fail.
#[cfg(unix)]
fn kill_descendant_pids(pids: &[u32]) {
    for &pid in pids {
        debug!("Killing descendant MCP process {}", pid);
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output();
    }
}

#[cfg(not(unix))]
fn kill_descendant_pids(_pids: &[u32]) {}

/// Read a single Content-Length framed message from a buffered reader.
async fn read_message<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R) -> McpResult<Vec<u8>> {
    let mut content_length: Option<usize> = None;

    // Read headers until we hit the empty line.
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(McpError::Transport("EOF while reading headers".to_string()));
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            // End of headers.
            break;
        }

        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|e| McpError::Protocol(format!("Invalid Content-Length: {}", e)))?,
            );
        }
        // Ignore other headers (e.g., Content-Type).
    }

    let length = content_length
        .ok_or_else(|| McpError::Protocol("Missing Content-Length header".to_string()))?;

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;

    Ok(body)
}

/// HTTP transport: communicates with a remote server via HTTP POST requests.
pub struct HttpTransport {
    url: String,
    #[allow(dead_code)]
    headers: HashMap<String, String>,
    client: reqwest::Client,
}

impl HttpTransport {
    pub fn new(url: String, headers: HashMap<String, String>) -> Self {
        let mut header_map = reqwest::header::HeaderMap::new();
        for (key, value) in &headers {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                reqwest::header::HeaderValue::from_str(value),
            ) {
                header_map.insert(name, val);
            }
        }

        let client = reqwest::Client::builder()
            .default_headers(header_map)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default();

        Self {
            url,
            headers,
            client,
        }
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn connect(&mut self) -> McpResult<()> {
        // HTTP is stateless — no persistent connection needed.
        Ok(())
    }

    async fn send_request(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        let response = self
            .client
            .post(&self.url)
            .json(request)
            .send()
            .await
            .map_err(|e| McpError::Transport(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(McpError::Transport(format!("HTTP {} — {}", status, body)));
        }

        let rpc_response = response.json::<JsonRpcResponse>().await?;
        Ok(rpc_response)
    }

    async fn send_notification(&self, notification: &JsonRpcNotification) -> McpResult<()> {
        self.client
            .post(&self.url)
            .json(notification)
            .send()
            .await
            .map_err(|e| McpError::Transport(format!("HTTP notification failed: {}", e)))?;
        Ok(())
    }

    async fn close(&self) -> McpResult<()> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn transport_type(&self) -> &str {
        "http"
    }
}

/// SSE transport: communicates with a server using Server-Sent Events.
pub struct SseTransport {
    url: String,
    #[allow(dead_code)]
    headers: HashMap<String, String>,
    client: reqwest::Client,
}

impl SseTransport {
    pub fn new(url: String, headers: HashMap<String, String>) -> Self {
        let mut header_map = reqwest::header::HeaderMap::new();
        for (key, value) in &headers {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                reqwest::header::HeaderValue::from_str(value),
            ) {
                header_map.insert(name, val);
            }
        }

        let client = reqwest::Client::builder()
            .default_headers(header_map)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default();

        Self {
            url,
            headers,
            client,
        }
    }
}

#[async_trait]
impl McpTransport for SseTransport {
    async fn connect(&mut self) -> McpResult<()> {
        Ok(())
    }

    async fn send_request(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        // SSE transport sends requests via POST and receives responses via SSE stream.
        // For now, use simple HTTP POST (full SSE streaming is a larger implementation).
        let response = self
            .client
            .post(&self.url)
            .json(request)
            .send()
            .await
            .map_err(|e| McpError::Transport(format!("SSE request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(McpError::Transport(format!(
                "SSE HTTP {} — {}",
                status, body
            )));
        }

        let rpc_response = response.json::<JsonRpcResponse>().await?;
        Ok(rpc_response)
    }

    async fn send_notification(&self, notification: &JsonRpcNotification) -> McpResult<()> {
        self.client
            .post(&self.url)
            .json(notification)
            .send()
            .await
            .map_err(|e| McpError::Transport(format!("SSE notification failed: {}", e)))?;
        Ok(())
    }

    async fn close(&self) -> McpResult<()> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn transport_type(&self) -> &str {
        "sse"
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
mod tests {
    use super::*;

    #[test]
    fn test_create_http_transport() {
        let config = McpServerConfig {
            url: Some("https://example.com/mcp".to_string()),
            transport: TransportType::Http,
            ..Default::default()
        };

        let transport = create_transport(&config).unwrap();
        assert_eq!(transport.transport_type(), "http");
    }

    #[test]
    fn test_create_sse_transport() {
        let config = McpServerConfig {
            url: Some("https://example.com/sse".to_string()),
            transport: TransportType::Sse,
            ..Default::default()
        };

        let transport = create_transport(&config).unwrap();
        assert_eq!(transport.transport_type(), "sse");
    }

    #[test]
    fn test_create_stdio_transport() {
        let config = McpServerConfig {
            command: "npx".to_string(),
            args: vec!["mcp-server-test".to_string()],
            transport: TransportType::Stdio,
            ..Default::default()
        };

        let transport = create_transport(&config).unwrap();
        assert_eq!(transport.transport_type(), "stdio");
    }

    #[test]
    fn test_http_without_url_fails() {
        let config = McpServerConfig {
            transport: TransportType::Http,
            ..Default::default()
        };

        assert!(create_transport(&config).is_err());
    }

    #[test]
    fn test_stdio_without_command_fails() {
        let config = McpServerConfig {
            transport: TransportType::Stdio,
            ..Default::default()
        };

        assert!(create_transport(&config).is_err());
    }

    #[test]
    fn test_npx_without_args_fails() {
        let config = McpServerConfig {
            command: "npx".to_string(),
            args: vec![],
            transport: TransportType::Stdio,
            ..Default::default()
        };

        assert!(create_transport(&config).is_err());
    }

    #[test]
    fn test_stdio_transport_not_connected() {
        let transport = StdioTransport::new(
            "node".to_string(),
            vec!["server.js".to_string()],
            HashMap::new(),
        );
        assert!(!transport.is_connected());
        assert_eq!(transport.command(), "node");
        assert_eq!(transport.args(), &["server.js"]);
    }

    #[tokio::test]
    async fn test_read_message_basic() {
        let input = b"Content-Length: 14\r\n\r\n{\"hello\":true}";
        let mut reader = tokio::io::BufReader::new(&input[..]);
        let body = read_message(&mut reader).await.unwrap();
        assert_eq!(body, b"{\"hello\":true}");
    }

    #[tokio::test]
    async fn test_read_message_with_extra_header() {
        let input = b"Content-Length: 2\r\nContent-Type: application/json\r\n\r\n{}";
        let mut reader = tokio::io::BufReader::new(&input[..]);
        let body = read_message(&mut reader).await.unwrap();
        assert_eq!(body, b"{}");
    }

    #[tokio::test]
    async fn test_read_message_missing_content_length() {
        let input = b"X-Custom: foo\r\n\r\n{}";
        let mut reader = tokio::io::BufReader::new(&input[..]);
        let result = read_message(&mut reader).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stdio_connect_and_echo() {
        // Spawn a simple cat-like echo process that reads Content-Length
        // framed messages and writes them back. We use a small Python script.
        let script = r#"
import sys
while True:
    line = sys.stdin.readline()
    if not line:
        break
    if line.startswith("Content-Length:"):
        length = int(line.split(":")[1].strip())
        sys.stdin.readline()  # empty line
        body = sys.stdin.read(length)
        response = body
        header = f"Content-Length: {len(response)}\r\n\r\n"
        sys.stdout.write(header)
        sys.stdout.write(response)
        sys.stdout.flush()
"#;

        let mut transport = StdioTransport::new(
            "python3".to_string(),
            vec!["-c".to_string(), script.to_string()],
            HashMap::new(),
        );

        transport.connect().await.unwrap();
        assert!(transport.is_connected());

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "test".to_string(),
            params: None,
        };

        let response = transport.send_request(&request).await.unwrap();
        assert_eq!(response.jsonrpc, "2.0");

        transport.close().await.unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn test_collect_descendant_pids_self_process() {
        // Our own PID should have no children in this test context.
        let my_pid = std::process::id();
        let descendants = collect_descendant_pids(my_pid);
        // We can't assert exact count (test runner may have threads),
        // but the function should not panic or hang.
        assert!(
            descendants.len() < 100,
            "Unreasonable number of descendants"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_collect_descendant_pids_nonexistent() {
        // A PID that almost certainly doesn't exist.
        let descendants = collect_descendant_pids(999_999_999);
        assert!(descendants.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn test_kill_descendant_pids_empty() {
        // Should be a no-op, no panics.
        kill_descendant_pids(&[]);
    }
}
