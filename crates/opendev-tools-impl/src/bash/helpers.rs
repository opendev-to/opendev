//! Utility functions for the bash tool.
//!
//! Output truncation, command preparation, failure diagnostics,
//! process group management, and background process tracking.

use std::collections::HashMap;
use std::sync::Arc;

use regex::Regex;
use tokio::sync::Mutex;
use tokio::time::Instant;

use super::patterns::needs_auto_confirm;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Idle timeout: kill when no stdout/stderr activity for this long.
pub(crate) const IDLE_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(60);

/// Absolute max runtime (safety cap).
pub(crate) const MAX_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(600);

/// Default timeout passed by callers (overridden by dual-timeout logic).
pub(crate) const DEFAULT_TIMEOUT_SECS: u64 = 120;

// Output truncation — display limits
pub(crate) const MAX_OUTPUT_CHARS: usize = 30_000;
const KEEP_HEAD_CHARS: usize = 10_000;
const KEEP_TAIL_CHARS: usize = 10_000;

// Output truncation — LLM metadata limits (more compact)
const MAX_LLM_METADATA_CHARS: usize = 15_000;
const LLM_KEEP_HEAD_CHARS: usize = 5_000;
const LLM_KEEP_TAIL_CHARS: usize = 5_000;

// ---------------------------------------------------------------------------
// Output truncation
// ---------------------------------------------------------------------------

/// Truncate by keeping head + tail, removing the middle.
pub(crate) fn truncate_output(text: &str, for_llm: bool) -> String {
    let (max, head, tail) = if for_llm {
        (
            MAX_LLM_METADATA_CHARS,
            LLM_KEEP_HEAD_CHARS,
            LLM_KEEP_TAIL_CHARS,
        )
    } else {
        (MAX_OUTPUT_CHARS, KEEP_HEAD_CHARS, KEEP_TAIL_CHARS)
    };
    if text.len() <= max {
        return text.to_string();
    }
    let removed = text.len() - head - tail;
    // Use char-boundary-safe slicing
    let head_str = safe_slice(text, 0, head);
    let tail_str = safe_slice(text, text.len() - tail, text.len());
    format!("{head_str}\n[...truncated {removed} chars...]\n{tail_str}")
}

/// Slice a string at char boundaries.
fn safe_slice(s: &str, start: usize, end: usize) -> &str {
    let start = s.floor_char_boundary(start);
    let end = s.floor_char_boundary(end);
    &s[start..end]
}

// ---------------------------------------------------------------------------
// Background process info
// ---------------------------------------------------------------------------

/// Tracked background process.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct BackgroundProcess {
    /// Unique ID for this background process.
    pub id: u32,
    /// Original command string.
    pub command: String,
    /// OS process ID.
    pub pid: u32,
    /// Process group ID (for clean kill).
    pub pgid: u32,
    /// When the process was started.
    pub started_at: Instant,
    /// Captured stdout lines so far.
    pub stdout_lines: Vec<String>,
    /// Captured stderr lines so far.
    pub stderr_lines: Vec<String>,
    /// Process handle (to poll exit status).
    pub child: tokio::process::Child,
}

/// Shared state for background processes.
pub(crate) type BackgroundStore = Arc<Mutex<HashMap<u32, BackgroundProcess>>>;

// ---------------------------------------------------------------------------
// LLM suffix for command failures (hidden from UI, visible to LLM)
// ---------------------------------------------------------------------------

pub(crate) fn command_failure_suffix(exit_code: i32, output: &str) -> String {
    let lower = output.to_lowercase();

    if lower.contains("permission denied") {
        "The command failed due to a permission error. Try using sudo or check file permissions."
            .to_string()
    } else if lower.contains("command not found") || lower.contains("no such file or directory") {
        format!(
            "The command failed (exit code {exit_code}). Check that the command/path exists \
             and is spelled correctly. Use `which` or `ls` to verify."
        )
    } else if lower.contains("syntax error") || lower.contains("unexpected token") {
        "The command had a syntax error. Review the command for typos or missing quotes/brackets."
            .to_string()
    } else if exit_code == 1 && (lower.contains("error") || lower.contains("failed")) {
        format!(
            "The command failed (exit code {exit_code}). Read the error output carefully, \
             then fix the issue and retry."
        )
    } else if exit_code == 2 {
        format!(
            "The command failed (exit code {exit_code}, typically misuse of shell command). \
             Check the command arguments and flags."
        )
    } else if exit_code == 126 {
        "The command was found but is not executable. Check file permissions with `ls -la`."
            .to_string()
    } else if exit_code == 127 {
        "The command was not found. Check spelling or install the missing tool.".to_string()
    } else if exit_code == 128 + 9 || exit_code == 128 + 15 {
        "The process was killed (likely OOM or external signal). Try reducing resource usage."
            .to_string()
    } else {
        format!(
            "The command failed with exit code {exit_code}. Read the error output, \
             diagnose the root cause, and try a corrected approach."
        )
    }
}

// ---------------------------------------------------------------------------
// Prepare command string (auto-confirm, python -u)
// ---------------------------------------------------------------------------

pub(crate) fn prepare_command(command: &str) -> String {
    let mut cmd = command.to_string();

    // Insert -u flag for python commands if not already present
    if let Ok(re) = Regex::new(r"^(python3?)\s+")
        && re.is_match(&cmd)
        && !cmd.contains(" -u ")
    {
        cmd = re.replace(&cmd, "${1} -u ").to_string();
    }

    // Wrap interactive commands with yes |
    if needs_auto_confirm(&cmd) {
        cmd = format!("yes | {cmd}");
    }

    cmd
}

// ---------------------------------------------------------------------------
// Kill an entire process group
// ---------------------------------------------------------------------------

pub(crate) fn kill_process_group(pgid: u32) {
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(-(pgid as i32), libc::SIGTERM);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        unsafe {
            let alive = libc::kill(-(pgid as i32), 0) == 0;
            if alive {
                libc::kill(-(pgid as i32), libc::SIGKILL);
            }
        }
    }
    #[cfg(windows)]
    {
        // On Windows, kill the process tree via `taskkill /F /T /PID`
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pgid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(test)]
#[path = "helpers_tests.rs"]
mod tests;
