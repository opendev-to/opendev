//! Callback HTTP server for `lm_query()` bridge.
//!
//! The sandbox Python code calls back to this server to make LLM queries.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use axum::Json;
use axum::routing::post;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::errors::{Result, SandboxError};

// ── Request / Response types ──

#[derive(Debug, Deserialize)]
pub struct LmQueryRequest {
    pub prompt: String,
    #[serde(default)]
    pub context: String,
}

#[derive(Debug, Serialize)]
pub struct LmQueryResponse {
    pub result: String,
}

#[derive(Debug, Deserialize)]
pub struct LmQueryBatchRequest {
    pub queries: Vec<LmQueryRequest>,
}

#[derive(Debug, Serialize)]
pub struct LmQueryBatchResponse {
    pub results: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ── Server ──

/// HTTP callback server that handles `lm_query()` calls from the sandbox.
pub struct CallbackServer {
    port: u16,
    _shutdown_tx: oneshot::Sender<()>,
    handle: JoinHandle<()>,
    query_count: Arc<AtomicU32>,
}

impl CallbackServer {
    /// Start the callback server on a random available port.
    pub async fn start(max_lm_queries: u32) -> Result<Self> {
        let query_count = Arc::new(AtomicU32::new(0));
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| SandboxError::CallbackServer(format!("Failed to bind: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| SandboxError::CallbackServer(format!("Failed to get port: {e}")))?
            .port();

        let qc = Arc::clone(&query_count);

        let app = axum::Router::new()
            .route(
                "/lm_query",
                post(move |Json(req): Json<LmQueryRequest>| {
                    let qc = Arc::clone(&qc);
                    async move {
                        let count = qc.fetch_add(1, Ordering::Relaxed) + 1;
                        if count > max_lm_queries {
                            return Json(serde_json::json!({
                                "error": format!("lm_query limit exceeded: {count}/{max_lm_queries}")
                            }));
                        }

                        // TODO: Wire to AdaptedClient::post_json() for real LLM calls.
                        // For now, return a placeholder.
                        let _ = req;
                        Json(serde_json::json!({
                            "result": "LLM response placeholder"
                        }))
                    }
                }),
            )
            .route(
                "/lm_query_batch",
                post(|Json(req): Json<LmQueryBatchRequest>| async move {
                    // TODO: Wire to AdaptedClient for parallel LLM calls.
                    let results: Vec<String> = req
                        .queries
                        .iter()
                        .map(|_| "LLM response placeholder".to_string())
                        .collect();
                    Json(serde_json::json!({ "results": results }))
                }),
            );

        let handle = tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            });
            if let Err(e) = server.await {
                warn!(error = %e, "Callback server error");
            }
        });

        info!(port, "Sandbox callback server started");

        Ok(Self {
            port,
            _shutdown_tx: shutdown_tx,
            handle,
            query_count,
        })
    }

    /// The port the callback server is listening on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Current number of `lm_query()` calls made.
    pub fn query_count(&self) -> u32 {
        self.query_count.load(Ordering::Relaxed)
    }

    /// Stop the callback server.
    pub async fn stop(self) {
        // Dropping _shutdown_tx signals the server to shut down.
        drop(self._shutdown_tx);
        let _ = self.handle.await;
        info!("Sandbox callback server stopped");
    }
}

impl std::fmt::Debug for CallbackServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallbackServer")
            .field("port", &self.port)
            .field("query_count", &self.query_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_callback_server_starts_and_stops() {
        let server = CallbackServer::start(10).await.unwrap();
        assert!(server.port() > 0);
        assert_eq!(server.query_count(), 0);
        server.stop().await;
    }
}
