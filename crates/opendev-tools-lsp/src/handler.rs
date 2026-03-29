//! Language server handler: JSON-RPC communication over stdin/stdout.
//!
//! Manages a single language server process, handling initialization,
//! request/response correlation, and shutdown.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{debug, error, warn};

use crate::error::LspError;
use crate::servers::ServerConfig;

/// A pending request awaiting a response.
type PendingRequest = oneshot::Sender<Result<Value, LspError>>;

/// Manages JSON-RPC communication with a language server process.
pub struct LspHandler {
    /// The language server process.
    process: Option<Child>,
    /// Stdin writer for sending requests.
    stdin_tx: Option<mpsc::Sender<String>>,
    /// Pending requests keyed by request ID.
    pending: Arc<Mutex<HashMap<i64, PendingRequest>>>,
    /// Next request ID counter.
    next_id: AtomicI64,
    /// Whether the server has been initialized.
    initialized: bool,
    /// Server configuration.
    config: ServerConfig,
    /// Workspace root for this server instance.
    workspace_root: PathBuf,
}

impl LspHandler {
    /// Create a new handler for a language server.
    pub fn new(config: ServerConfig, workspace_root: PathBuf) -> Self {
        Self {
            process: None,
            stdin_tx: None,
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicI64::new(1),
            initialized: false,
            config,
            workspace_root,
        }
    }

    /// Start the language server process and begin initialization.
    pub async fn start(&mut self) -> Result<(), LspError> {
        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            LspError::ServerStart(format!("Failed to start {}: {}", self.config.command, e))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::ServerStart("Failed to capture stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::ServerStart("Failed to capture stdout".to_string()))?;

        // Spawn stdin writer task
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(64);
        tokio::spawn(async move {
            let mut writer = stdin;
            while let Some(msg) = stdin_rx.recv().await {
                let header = format!("Content-Length: {}\r\n\r\n", msg.len());
                if let Err(e) = writer.write_all(header.as_bytes()).await {
                    error!("Failed to write header: {}", e);
                    break;
                }
                if let Err(e) = writer.write_all(msg.as_bytes()).await {
                    error!("Failed to write body: {}", e);
                    break;
                }
                if let Err(e) = writer.flush().await {
                    error!("Failed to flush: {}", e);
                    break;
                }
            }
        });

        // Spawn stdout reader task
        let pending = Arc::clone(&self.pending);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader).await {
                    Ok(Some(msg)) => {
                        if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
                            // This is a response
                            let mut pending = pending.lock().await;
                            if let Some(sender) = pending.remove(&id) {
                                if let Some(error) = msg.get("error") {
                                    let err_msg = error
                                        .get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("Unknown error")
                                        .to_string();
                                    let _ = sender.send(Err(LspError::ServerResponse(err_msg)));
                                } else {
                                    let result = msg.get("result").cloned().unwrap_or(Value::Null);
                                    let _ = sender.send(Ok(result));
                                }
                            }
                        }
                        // Notifications (no id) are logged but not dispatched
                    }
                    Ok(None) => {
                        debug!("LSP stdout closed");
                        break;
                    }
                    Err(e) => {
                        warn!("Error reading LSP message: {}", e);
                        break;
                    }
                }
            }
        });

        self.process = Some(child);
        self.stdin_tx = Some(stdin_tx);

        // Send initialize request
        self.initialize().await?;

        Ok(())
    }

    /// Send the LSP initialize request.
    async fn initialize(&mut self) -> Result<(), LspError> {
        let uri = crate::protocol::path_to_uri_string(&self.workspace_root)
            .map(|u: String| Value::String(u))
            .unwrap_or(Value::Null);

        let params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": uri,
            "capabilities": {
                "textDocument": {
                    "documentSymbol": {
                        "hierarchicalDocumentSymbolSupport": true
                    },
                    "references": {},
                    "rename": {
                        "prepareSupport": true
                    },
                    "definition": {},
                    "hover": {}
                },
                "workspace": {
                    "symbol": {
                        "dynamicRegistration": false
                    },
                    "workspaceEdit": {
                        "documentChanges": true
                    }
                }
            }
        });

        let result = self.send_request("initialize", params).await?;
        debug!(
            "LSP initialized: {:?}",
            result.get("capabilities").is_some()
        );

        // Send initialized notification
        self.send_notification("initialized", serde_json::json!({}))
            .await?;

        self.initialized = true;
        Ok(())
    }

    /// Send a JSON-RPC request and wait for the response.
    pub async fn send_request(&self, method: &str, params: Value) -> Result<Value, LspError> {
        let tx = self.stdin_tx.as_ref().ok_or(LspError::NotRunning)?;

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let (response_tx, response_rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, response_tx);
        }

        let msg = serde_json::to_string(&request)
            .map_err(|e| LspError::Protocol(format!("Serialize error: {}", e)))?;

        tx.send(msg).await.map_err(|_| LspError::NotRunning)?;

        // Wait for response with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(30), response_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(LspError::ServerResponse("Response channel dropped".into())),
            Err(_) => {
                // Clean up the pending request
                let mut pending = self.pending.lock().await;
                pending.remove(&id);
                Err(LspError::Timeout(method.to_string()))
            }
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), LspError> {
        let tx = self.stdin_tx.as_ref().ok_or(LspError::NotRunning)?;

        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let msg = serde_json::to_string(&notification)
            .map_err(|e| LspError::Protocol(format!("Serialize error: {}", e)))?;

        tx.send(msg).await.map_err(|_| LspError::NotRunning)?;
        Ok(())
    }

    /// Whether the server is running and initialized.
    pub fn is_ready(&self) -> bool {
        self.initialized && self.stdin_tx.is_some()
    }

    /// Shutdown the language server gracefully.
    pub async fn shutdown(&mut self) -> Result<(), LspError> {
        if !self.is_ready() {
            return Ok(());
        }

        // Send shutdown request
        let _ = self.send_request("shutdown", Value::Null).await;

        // Send exit notification
        let _ = self.send_notification("exit", Value::Null).await;

        self.initialized = false;
        self.stdin_tx = None;

        // Wait for process to exit
        if let Some(ref mut child) = self.process {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;
        }

        self.process = None;
        Ok(())
    }

    /// Get the workspace root.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Get the server config.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }
}

impl Drop for LspHandler {
    fn drop(&mut self) {
        // Best-effort cleanup — can't do async in Drop
        self.stdin_tx = None;
        if let Some(ref mut child) = self.process {
            let _ = child.start_kill();
        }
    }
}

/// Read a single JSON-RPC message from the reader.
///
/// Parses `Content-Length` headers and reads the body.
async fn read_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<Value>, LspError> {
    let mut content_length: Option<usize> = None;

    // Read headers
    loop {
        let mut header_line = String::new();
        let n = reader
            .read_line(&mut header_line)
            .await
            .map_err(|e| LspError::Protocol(format!("Read header error: {}", e)))?;

        if n == 0 {
            return Ok(None); // EOF
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break; // End of headers
        }

        if let Some(value) = trimmed
            .strip_prefix("Content-Length:")
            .or_else(|| trimmed.strip_prefix("content-length:"))
        {
            content_length = value.trim().parse().ok();
        }
    }

    let length = content_length
        .ok_or_else(|| LspError::Protocol("Missing Content-Length header".to_string()))?;

    // Read body
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|e| LspError::Protocol(format!("Read body error: {}", e)))?;

    let value: Value = serde_json::from_slice(&body)
        .map_err(|e| LspError::Protocol(format!("Parse JSON error: {}", e)))?;

    Ok(Some(value))
}

impl std::fmt::Debug for LspHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LspHandler")
            .field("config", &self.config)
            .field("workspace_root", &self.workspace_root)
            .field("initialized", &self.initialized)
            .finish()
    }
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
