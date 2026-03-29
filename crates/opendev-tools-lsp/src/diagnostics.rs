//! LSP diagnostics debouncing.
//!
//! Language servers can emit rapid-fire `textDocument/publishDiagnostics`
//! notifications — for example on every keystroke in the editor. Processing
//! each one individually is wasteful.
//!
//! [`DiagnosticsDebouncer`] batches these updates: it collects all diagnostics
//! that arrive within a configurable window (default 100 ms) and then delivers
//! a single merged snapshot per file.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, mpsc};
use tracing::debug;

/// Default debounce window.
const DEFAULT_DEBOUNCE_MS: u64 = 100;

/// A single diagnostic from the language server.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    /// File path the diagnostic applies to.
    pub file_path: PathBuf,
    /// 0-based line number.
    pub line: u32,
    /// 0-based column.
    pub character: u32,
    /// Severity: 1 = Error, 2 = Warning, 3 = Info, 4 = Hint.
    pub severity: u32,
    /// Diagnostic message.
    pub message: String,
    /// Source of the diagnostic (e.g., "rustc", "pyright").
    pub source: Option<String>,
}

/// A batch of diagnostics for a single file after debouncing.
#[derive(Debug, Clone)]
pub struct DiagnosticsBatch {
    /// File this batch belongs to.
    pub file_path: PathBuf,
    /// All diagnostics for the file (replaces any previous set).
    pub diagnostics: Vec<Diagnostic>,
}

/// Debounces rapid diagnostic updates from language servers.
///
/// Usage:
/// 1. Create with [`DiagnosticsDebouncer::new`].
/// 2. Call [`push`] whenever a `publishDiagnostics` notification arrives.
/// 3. Consume batched results from the receiver returned by [`new`].
pub struct DiagnosticsDebouncer {
    /// Pending diagnostics grouped by file path.
    pending: Arc<Mutex<HashMap<PathBuf, Vec<Diagnostic>>>>,
    /// Sender for raw (unbounced) diagnostic events.
    input_tx: mpsc::UnboundedSender<Diagnostic>,
    /// Handle for the background debounce task.
    task_handle: Option<tokio::task::JoinHandle<()>>,
    /// Debounce window duration.
    debounce_duration: Duration,
}

impl DiagnosticsDebouncer {
    /// Create a new debouncer with the default 100 ms window.
    ///
    /// Returns the debouncer and a receiver for batched diagnostic results.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<DiagnosticsBatch>) {
        Self::with_duration(Duration::from_millis(DEFAULT_DEBOUNCE_MS))
    }

    /// Create a new debouncer with a custom debounce window.
    pub fn with_duration(duration: Duration) -> (Self, mpsc::UnboundedReceiver<DiagnosticsBatch>) {
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let (output_tx, output_rx) = mpsc::unbounded_channel();
        let pending: Arc<Mutex<HashMap<PathBuf, Vec<Diagnostic>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_clone = Arc::clone(&pending);
        let debounce_duration = duration;

        let handle = tokio::spawn(Self::debounce_loop(
            input_rx,
            output_tx,
            pending_clone,
            debounce_duration,
        ));

        (
            Self {
                pending,
                input_tx,
                task_handle: Some(handle),
                debounce_duration,
            },
            output_rx,
        )
    }

    /// Push a diagnostic update into the debouncer.
    ///
    /// The diagnostic will be batched with other updates for the same file
    /// within the debounce window.
    pub fn push(&self, diagnostic: Diagnostic) -> Result<(), mpsc::error::SendError<Diagnostic>> {
        self.input_tx.send(diagnostic)
    }

    /// Push a full set of diagnostics for a file (replaces any pending for that file).
    pub fn push_file_diagnostics(&self, file_path: PathBuf, diagnostics: Vec<Diagnostic>) {
        // We push them individually through the channel; the debounce loop
        // will batch them. We first clear pending for this file.
        let pending = self.pending.clone();
        let tx = self.input_tx.clone();
        tokio::spawn(async move {
            {
                let mut p = pending.lock().await;
                p.insert(file_path.clone(), Vec::new());
            }
            for diag in diagnostics {
                let _ = tx.send(diag);
            }
        });
    }

    /// Get the debounce duration.
    pub fn debounce_duration(&self) -> Duration {
        self.debounce_duration
    }

    /// Stop the debouncer and its background task.
    pub fn stop(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
    }

    /// The background debounce loop.
    ///
    /// Collects incoming diagnostics, waits for the debounce window to
    /// elapse with no new events for a file, then flushes as a batch.
    async fn debounce_loop(
        mut input_rx: mpsc::UnboundedReceiver<Diagnostic>,
        output_tx: mpsc::UnboundedSender<DiagnosticsBatch>,
        pending: Arc<Mutex<HashMap<PathBuf, Vec<Diagnostic>>>>,
        debounce_duration: Duration,
    ) {
        loop {
            // Wait for the first diagnostic.
            let first = match input_rx.recv().await {
                Some(d) => d,
                None => break, // Channel closed.
            };

            // Add to pending.
            {
                let mut p = pending.lock().await;
                p.entry(first.file_path.clone()).or_default().push(first);
            }

            // Collect more diagnostics within the debounce window.
            let deadline = tokio::time::Instant::now() + debounce_duration;
            loop {
                match tokio::time::timeout_at(deadline, input_rx.recv()).await {
                    Ok(Some(diag)) => {
                        let mut p = pending.lock().await;
                        p.entry(diag.file_path.clone()).or_default().push(diag);
                    }
                    Ok(None) => {
                        // Channel closed.
                        return;
                    }
                    Err(_) => {
                        // Timeout: debounce window elapsed.
                        break;
                    }
                }
            }

            // Flush all pending batches.
            let batches: Vec<DiagnosticsBatch> = {
                let mut p = pending.lock().await;
                let batches: Vec<_> = p
                    .drain()
                    .map(|(file_path, diagnostics)| DiagnosticsBatch {
                        file_path,
                        diagnostics,
                    })
                    .collect();
                batches
            };

            for batch in batches {
                debug!(
                    file = %batch.file_path.display(),
                    count = batch.diagnostics.len(),
                    "Flushing debounced diagnostics batch"
                );
                if output_tx.send(batch).is_err() {
                    return; // Receiver dropped.
                }
            }
        }
    }
}

impl Drop for DiagnosticsDebouncer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
