//! Stdio transport: communicates with a child process via stdin/stdout.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, oneshot};
use tokio::time::{Duration, timeout};
use tracing::{debug, trace, warn};

use super::McpTransport;
use super::REQUEST_TIMEOUT;
use super::framing::read_message;
use super::process::{collect_descendant_pids, kill_descendant_pids};
use crate::error::{McpError, McpResult};
use crate::models::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

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

#[cfg(test)]
#[path = "stdio_tests.rs"]
mod tests;
