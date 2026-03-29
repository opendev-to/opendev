//! Git-based snapshot system for reliable file change tracking and revert.
//!
//! Uses a shadow git repository to create atomic snapshots of modified files
//! before and after tool execution. Enables reliable revert of any change set.

use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::{debug, warn};

/// Manages file snapshots using a shadow git repository.
pub struct SnapshotManager {
    project_dir: PathBuf,
    snapshot_dir: PathBuf,
    initialized: bool,
}

impl SnapshotManager {
    /// Create a new snapshot manager for the given project directory.
    pub fn new(project_dir: &Path) -> Self {
        let project_dir = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf());
        let project_id = compute_project_id(&project_dir);

        let snapshot_dir = dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".opendev")
            .join("data")
            .join("snapshot")
            .join(&project_id);

        Self {
            project_dir,
            snapshot_dir,
            initialized: false,
        }
    }

    /// Path to the snapshot directory.
    pub fn snapshot_dir(&self) -> &Path {
        &self.snapshot_dir
    }

    /// Take a snapshot of the given files before modification.
    ///
    /// Returns the snapshot ID (git commit hash) or `None` on failure.
    pub fn take_snapshot(&mut self, files: &[&str], label: &str) -> Option<String> {
        if !self.ensure_initialized() {
            return None;
        }

        let mut copied = 0usize;

        for file_path in files {
            let src = Path::new(file_path);
            if !src.exists() {
                continue;
            }

            let rel = src
                .strip_prefix(&self.project_dir)
                .unwrap_or_else(|_| Path::new(src.file_name().unwrap_or_default()));

            let dest = self.snapshot_dir.join(rel);
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            match std::fs::copy(src, &dest) {
                Ok(_) => {
                    self.git(&["add", &rel.to_string_lossy()]);
                    copied += 1;
                }
                Err(e) => {
                    debug!("Failed to copy {}: {}", src.display(), e);
                }
            }
        }

        if copied == 0 {
            return None;
        }

        let msg = if label.is_empty() {
            "snapshot".to_string()
        } else {
            format!("snapshot: {label}")
        };

        self.git(&["commit", "-m", &msg, "--allow-empty"]);

        let output = self.git(&["rev-parse", "HEAD"])?;
        let snapshot_id = output.trim().to_string();

        debug!(
            "Snapshot {}: {} files ({})",
            &snapshot_id[..8.min(snapshot_id.len())],
            copied,
            label
        );

        Some(snapshot_id)
    }

    /// Get diff between a snapshot and the current project state.
    pub fn get_diff(&mut self, snapshot_id: &str) -> Option<String> {
        if !self.ensure_initialized() {
            return None;
        }

        let output = self.git(&[
            "diff-tree",
            "--no-commit-id",
            "-r",
            "--name-only",
            snapshot_id,
        ])?;
        let files: Vec<&str> = output.lines().filter(|l| !l.is_empty()).collect();

        for rel_path in &files {
            let src = self.project_dir.join(rel_path);
            let dest = self.snapshot_dir.join(rel_path);

            if src.exists() {
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::copy(&src, &dest);
            } else if dest.exists() {
                let _ = std::fs::remove_file(&dest);
            }
        }

        self.git(&["diff", snapshot_id, "--"])
    }

    /// Revert project files to a snapshot state.
    ///
    /// Returns list of reverted file paths.
    pub fn revert_to_snapshot(&mut self, snapshot_id: &str) -> Vec<String> {
        if !self.ensure_initialized() {
            return Vec::new();
        }

        let output = match self.git(&[
            "diff-tree",
            "--no-commit-id",
            "-r",
            "--name-only",
            snapshot_id,
        ]) {
            Some(o) => o,
            None => return Vec::new(),
        };

        let mut reverted = Vec::new();

        for rel_path in output.lines().filter(|l| !l.is_empty()) {
            self.git(&["checkout", snapshot_id, "--", rel_path]);

            let src = self.snapshot_dir.join(rel_path);
            let dest = self.project_dir.join(rel_path);

            if src.exists() {
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::copy(&src, &dest) {
                    Ok(_) => reverted.push(dest.to_string_lossy().to_string()),
                    Err(e) => warn!("Failed to revert {}: {}", rel_path, e),
                }
            }
        }

        reverted
    }

    /// Run garbage collection on the snapshot repo.
    pub fn cleanup(&mut self, max_age_days: u32) {
        if !self.ensure_initialized() {
            return;
        }
        let prune_arg = format!("--prune={max_age_days}.days.ago");
        if self.git(&["gc", &prune_arg]).is_none() {
            debug!("Snapshot GC failed");
        }
    }

    fn ensure_initialized(&mut self) -> bool {
        if self.initialized {
            return true;
        }

        if std::fs::create_dir_all(&self.snapshot_dir).is_err() {
            warn!("Failed to create snapshot dir");
            return false;
        }

        let git_dir = self.snapshot_dir.join(".git");
        if !git_dir.exists() {
            if self.git(&["init"]).is_none() {
                return false;
            }
            self.git(&["config", "user.name", "opendev-snapshot"]);
            self.git(&["config", "user.email", "snapshot@opendev.local"]);
            self.git(&["config", "gc.auto", "0"]);
        }

        self.initialized = true;
        true
    }

    fn git(&self, args: &[&str]) -> Option<String> {
        let result = Command::new("git")
            .args(args)
            .current_dir(&self.snapshot_dir)
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stderr.contains("nothing to commit") {
                        debug!("git {} failed: {}", args.join(" "), stderr.trim());
                        return None;
                    }
                }
                Some(String::from_utf8_lossy(&output.stdout).to_string())
            }
            Err(e) => {
                debug!("git command failed: {}", e);
                None
            }
        }
    }
}

fn compute_project_id(project_dir: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    project_dir.display().to_string().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

impl std::fmt::Debug for SnapshotManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnapshotManager")
            .field("project_dir", &self.project_dir)
            .field("snapshot_dir", &self.snapshot_dir)
            .field("initialized", &self.initialized)
            .finish()
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
