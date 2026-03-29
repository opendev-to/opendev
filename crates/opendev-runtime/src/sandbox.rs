//! Sandbox mode for restricting tool operations.
//!
//! When enabled, only whitelisted commands may be executed by the bash tool,
//! and file write/edit operations are restricted to paths within the project directory.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Configuration for sandbox mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Whether sandbox restrictions are active.
    pub enabled: bool,
    /// Commands allowed in the bash tool (matched by prefix, e.g. `"cargo"` allows `"cargo build"`).
    pub allowed_commands: Vec<String>,
    /// Paths (directories) where file write/edit operations are permitted.
    /// Paths are normalized and checked via prefix matching.
    pub writable_paths: Vec<String>,
}

impl SandboxConfig {
    /// Create a disabled sandbox config.
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Create an enabled sandbox config with the given project directory as the sole writable path
    /// and a default set of safe commands.
    pub fn for_project(project_dir: &Path) -> Self {
        Self {
            enabled: true,
            allowed_commands: vec![
                "cargo".into(),
                "rustc".into(),
                "npm".into(),
                "node".into(),
                "python".into(),
                "git".into(),
                "ls".into(),
                "cat".into(),
                "head".into(),
                "tail".into(),
                "grep".into(),
                "find".into(),
                "wc".into(),
                "sort".into(),
                "uniq".into(),
                "diff".into(),
                "echo".into(),
                "pwd".into(),
                "which".into(),
                "env".into(),
                "test".into(),
                "true".into(),
                "false".into(),
                "mkdir".into(),
                "cp".into(),
                "mv".into(),
                "touch".into(),
            ],
            writable_paths: vec![project_dir.to_string_lossy().to_string()],
        }
    }

    /// Check whether a bash command is allowed in sandbox mode.
    ///
    /// Returns `Ok(())` if sandbox is disabled or the command is whitelisted.
    /// Returns `Err` with a human-readable message if blocked.
    pub fn check_command(&self, command: &str) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        let trimmed = command.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        // Extract the base command (first word, stripping any env var prefix)
        let base_cmd = extract_base_command(trimmed);

        if self
            .allowed_commands
            .iter()
            .any(|allowed| base_cmd == allowed.as_str())
        {
            return Ok(());
        }

        Err(format!(
            "Sandbox: command '{}' is not in the allowed list. Allowed: {:?}",
            base_cmd, self.allowed_commands
        ))
    }

    /// Check whether a file path is writable in sandbox mode.
    ///
    /// Returns `Ok(())` if sandbox is disabled or the path is within a writable directory.
    /// Returns `Err` with a human-readable message if blocked.
    pub fn check_writable_path(&self, path: &Path) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        // Normalize the path for comparison
        let normalized = normalize_path(path);

        for writable in &self.writable_paths {
            let writable_normalized = normalize_path(Path::new(writable));
            if normalized.starts_with(&writable_normalized) {
                return Ok(());
            }
        }

        Err(format!(
            "Sandbox: path '{}' is not within writable directories. Writable: {:?}",
            path.display(),
            self.writable_paths
        ))
    }
}

/// Extract the base command name from a shell command string.
///
/// Handles:
/// - Leading env var assignments: `FOO=bar cmd args` -> `cmd`
/// - Leading path: `/usr/bin/cmd args` -> `cmd`
/// - Simple commands: `cmd args` -> `cmd`
fn extract_base_command(command: &str) -> &str {
    let mut parts = command.split_whitespace();

    // Skip env var assignments (KEY=VALUE)
    let cmd = loop {
        match parts.next() {
            Some(part) if part.contains('=') && !part.starts_with('-') => continue,
            Some(part) => break part,
            None => return "",
        }
    };

    // Strip path prefix: `/usr/bin/cargo` -> `cargo`
    cmd.rsplit('/').next().unwrap_or(cmd)
}

/// Best-effort path normalization without requiring the path to exist.
fn normalize_path(path: &Path) -> PathBuf {
    // Try canonical first (resolves symlinks, requires path to exist)
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    // Fall back to the path as-is but with components resolved
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            _ => result.push(component),
        }
    }
    result
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
