//! Middleware that captures file snapshots before edit/write tools execute.
//!
//! Registered on the tool registry so that before any file-modifying tool
//! runs, the current file content is saved for undo support.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use opendev_history::FileCheckpointManager;
use opendev_tools_core::middleware::ToolMiddleware;
use opendev_tools_core::traits::{ToolContext, ToolResult};
use tracing::warn;

/// Tool names that modify files and need pre-execution snapshots.
const FILE_WRITE_TOOLS: &[&str] = &["Edit", "Write", "multi_edit"];

#[derive(Debug)]
pub struct FileCheckpointMiddleware {
    manager: Arc<Mutex<FileCheckpointManager>>,
}

impl FileCheckpointMiddleware {
    pub fn new(manager: Arc<Mutex<FileCheckpointManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait::async_trait]
impl ToolMiddleware for FileCheckpointMiddleware {
    async fn before_execute(
        &self,
        name: &str,
        args: &HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> Result<(), String> {
        if !FILE_WRITE_TOOLS.contains(&name) {
            return Ok(());
        }

        let file_path = match args.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return Ok(()), // No file_path arg — let the tool handle validation
        };

        // Resolve to absolute path (same logic as the tools themselves)
        let abs_path = if file_path.starts_with('/') {
            PathBuf::from(file_path)
        } else {
            ctx.working_dir.join(file_path)
        };

        // Capture but never fail the tool
        if let Ok(mut mgr) = self.manager.lock()
            && let Err(e) = mgr.capture_file(&abs_path)
        {
            warn!(
                "Checkpoint capture failed for {}: {}",
                abs_path.display(),
                e
            );
        }

        Ok(())
    }

    async fn after_execute(&self, _name: &str, _result: &ToolResult) -> Result<(), String> {
        Ok(())
    }
}
