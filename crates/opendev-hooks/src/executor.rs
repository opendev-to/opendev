//! Subprocess runner for hook commands.
//!
//! Hooks are executed as shell subprocesses. The command receives JSON on stdin
//! and communicates results via exit codes and optional JSON on stdout.
//!
//! Exit codes:
//! - 0: Success (operation proceeds)
//! - 2: Block (operation is denied)
//! - Other: Error (logged, operation proceeds)

use crate::models::HookCommand;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{error, warn};

/// Result from executing a single hook command.
#[derive(Debug, Clone, Default)]
pub struct HookResult {
    /// Process exit code (0 = success, 2 = block).
    pub exit_code: i32,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Whether the command timed out.
    pub timed_out: bool,
    /// Error message if the command failed to execute.
    pub error: Option<String>,
}

impl HookResult {
    /// Hook succeeded (exit code 0, no timeout, no error).
    pub fn success(&self) -> bool {
        self.exit_code == 0 && !self.timed_out && self.error.is_none()
    }

    /// Hook requests blocking the operation (exit code 2).
    pub fn should_block(&self) -> bool {
        self.exit_code == 2
    }

    /// Parse stdout as a JSON object.
    ///
    /// Returns an empty map if stdout is empty or not valid JSON.
    pub fn parse_json_output(&self) -> HashMap<String, Value> {
        let trimmed = self.stdout.trim();
        if trimmed.is_empty() {
            return HashMap::new();
        }
        serde_json::from_str(trimmed).unwrap_or_default()
    }
}

/// Executes hook commands as subprocesses.
///
/// This is an async executor that spawns shell processes, pipes JSON on stdin,
/// and captures output with a timeout.
#[derive(Debug, Clone)]
pub struct HookExecutor;

impl HookExecutor {
    pub fn new() -> Self {
        Self
    }

    /// Execute a hook command.
    ///
    /// The command receives `stdin_data` as JSON on stdin and communicates
    /// results via exit code and optional JSON on stdout.
    pub async fn execute(&self, command: &HookCommand, stdin_data: &Value) -> HookResult {
        let stdin_json = match serde_json::to_string(stdin_data) {
            Ok(s) => s,
            Err(e) => {
                return HookResult {
                    exit_code: 1,
                    error: Some(format!("Failed to serialize stdin data: {e}")),
                    ..Default::default()
                };
            }
        };

        let timeout = Duration::from_secs(command.effective_timeout() as u64);

        // Determine shell to use
        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let mut child = match Command::new(shell)
            .arg(flag)
            .arg(&command.command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                error!(
                    command = %command.command,
                    error = %e,
                    "Hook command failed to execute"
                );
                return HookResult {
                    exit_code: 1,
                    error: Some(format!("Failed to execute hook: {e}")),
                    ..Default::default()
                };
            }
        };

        // Write stdin
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(stdin_json.as_bytes()).await {
                warn!(error = %e, "Failed to write stdin to hook command");
            }
            // Drop stdin to close the pipe so the child can read EOF
            drop(stdin);
        }

        // Read stdout/stderr handles before waiting (wait_with_output takes
        // ownership, so we use the lower-level approach to allow killing on timeout).
        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        // Wait with timeout
        match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) => {
                let exit_code = status.code().unwrap_or(1);

                // Read captured output
                let stdout = if let Some(mut out) = stdout_handle {
                    use tokio::io::AsyncReadExt;
                    let mut buf = Vec::new();
                    let _ = out.read_to_end(&mut buf).await;
                    String::from_utf8_lossy(&buf).to_string()
                } else {
                    String::new()
                };

                let stderr = if let Some(mut err) = stderr_handle {
                    use tokio::io::AsyncReadExt;
                    let mut buf = Vec::new();
                    let _ = err.read_to_end(&mut buf).await;
                    String::from_utf8_lossy(&buf).to_string()
                } else {
                    String::new()
                };

                HookResult {
                    exit_code,
                    stdout,
                    stderr,
                    timed_out: false,
                    error: None,
                }
            }
            Ok(Err(e)) => {
                error!(
                    command = %command.command,
                    error = %e,
                    "Hook command I/O error"
                );
                HookResult {
                    exit_code: 1,
                    error: Some(format!("Hook I/O error: {e}")),
                    ..Default::default()
                }
            }
            Err(_elapsed) => {
                warn!(
                    command = %command.command,
                    timeout_secs = command.effective_timeout(),
                    "Hook command timed out"
                );
                // Kill the child process
                let _ = child.kill().await;
                HookResult {
                    exit_code: 1,
                    timed_out: true,
                    error: Some(format!(
                        "Hook timed out after {}s",
                        command.effective_timeout()
                    )),
                    ..Default::default()
                }
            }
        }
    }
}

impl Default for HookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
