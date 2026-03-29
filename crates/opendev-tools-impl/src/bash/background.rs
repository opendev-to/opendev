//! Background (fire-and-forget) command execution with startup output capture.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use opendev_tools_core::ToolResult;

use super::BashTool;
use super::helpers::{BackgroundProcess, command_failure_suffix, prepare_command};
use super::patterns::filtered_env;

impl BashTool {
    pub(super) async fn run_background(
        &self,
        command: &str,
        working_dir: &std::path::Path,
    ) -> ToolResult {
        let exec_command = prepare_command(command);

        let safe_env = filtered_env();
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&exec_command)
            .current_dir(working_dir)
            .env_clear()
            .envs(&safe_env)
            .env("PYTHONUNBUFFERED", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Create new process group on Unix for clean kill
        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                libc::setpgid(0, 0);
                Ok(())
            });
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to spawn background command: {e}")),
        };

        let pid = child.id().unwrap_or(0);
        let pgid = pid;

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        // Capture initial startup output (up to 20s, with 3s idle timeout)
        let stdout_buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let stderr_buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let startup_activity = Arc::new(Mutex::new(Instant::now()));

        // Spawn stdout reader
        let stdout_reader_lines = stdout_buf.clone();
        let stdout_activity = startup_activity.clone();
        let stdout_reader = tokio::spawn(async move {
            if let Some(pipe) = stdout_pipe {
                let mut reader = BufReader::new(pipe).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    *stdout_activity.lock().await = Instant::now();
                    stdout_reader_lines.lock().await.push(line);
                }
            }
        });

        // Spawn stderr reader
        let stderr_reader_lines = stderr_buf.clone();
        let stderr_activity = startup_activity.clone();
        let stderr_reader = tokio::spawn(async move {
            if let Some(pipe) = stderr_pipe {
                let mut reader = BufReader::new(pipe).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    *stderr_activity.lock().await = Instant::now();
                    stderr_reader_lines.lock().await.push(line);
                }
            }
        });

        // Wait for startup output with idle timeout
        let startup_start = Instant::now();
        let max_startup = Duration::from_secs(20);
        let startup_idle = Duration::from_secs(3);

        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;

            // Check if child already exited
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Process finished during startup
                    let _ = tokio::time::timeout(Duration::from_secs(1), stdout_reader).await;
                    let _ = tokio::time::timeout(Duration::from_secs(1), stderr_reader).await;

                    let stdout_text = stdout_buf.lock().await.join("\n");
                    let stderr_text = stderr_buf.lock().await.join("\n");
                    let exit_code = status.code().unwrap_or(-1);

                    let mut combined = stdout_text;
                    if !stderr_text.is_empty() {
                        if !combined.is_empty() {
                            combined.push('\n');
                        }
                        combined.push_str(&format!("[stderr]\n{stderr_text}"));
                    }

                    let mut metadata = HashMap::new();
                    metadata.insert("exit_code".into(), serde_json::json!(exit_code));

                    if status.success() {
                        return ToolResult::ok_with_metadata(combined, metadata);
                    } else {
                        let suffix = command_failure_suffix(exit_code, &combined);
                        return ToolResult {
                            success: false,
                            output: Some(combined),
                            error: Some(format!("Command exited with code {exit_code}")),
                            metadata,
                            duration_ms: None,
                            llm_suffix: Some(suffix),
                        };
                    }
                }
                Ok(None) => {} // still running
                Err(_) => {}
            }

            // Check startup capture time limits
            if startup_start.elapsed() >= max_startup {
                break;
            }
            let idle_elapsed = startup_activity.lock().await.elapsed();
            // Give at least 1s before checking idle
            if startup_start.elapsed() > Duration::from_secs(1) && idle_elapsed >= startup_idle {
                break;
            }
        }

        // Process still running — store as background
        let bg_id = self.next_id().await;
        let stdout_captured = stdout_buf.lock().await.clone();
        let stderr_captured = stderr_buf.lock().await.clone();
        let startup_output = stdout_captured.join("\n");

        let bp = BackgroundProcess {
            id: bg_id,
            command: command.to_string(),
            pid,
            pgid,
            started_at: Instant::now(),
            stdout_lines: stdout_captured,
            stderr_lines: stderr_captured,
            child,
        };
        self.background.lock().await.insert(bg_id, bp);

        // Keep reader tasks alive — they'll stop when the child's pipes close.
        tokio::spawn(async move {
            let _ = stdout_reader.await;
        });
        tokio::spawn(async move {
            let _ = stderr_reader.await;
        });

        let mut metadata = HashMap::new();
        metadata.insert("background_id".into(), serde_json::json!(bg_id));
        metadata.insert("pid".into(), serde_json::json!(pid));

        let msg = if startup_output.is_empty() {
            format!("Background process started (id={bg_id}, pid={pid})")
        } else {
            format!(
                "Background process started (id={bg_id}, pid={pid})\n\
                 Startup output:\n{startup_output}"
            )
        };

        ToolResult::ok_with_metadata(msg, metadata)
    }
}
