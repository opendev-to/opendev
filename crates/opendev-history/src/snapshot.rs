//! Shadow git snapshot system for per-step undo.
//!
//! Maintains a parallel shadow git repository at `~/.opendev/snapshot/<project_id>/`
//! that captures a tree hash at every agent step, enabling perfect per-step
//! undo/revert without touching the user's real git repo.

use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::{debug, info, warn};

/// Create a stable, filesystem-safe ID from a project path.
fn encode_project_id(project_path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    project_path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Line-level diff stats for a single file.
#[derive(Debug, Clone)]
pub struct FileDiffStat {
    pub file_path: String,
    pub additions: u64,
    pub deletions: u64,
    pub is_binary: bool,
}

/// Full diff information for a single file, including content.
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub file_path: String,
    pub before: String,
    pub after: String,
    pub additions: u64,
    pub deletions: u64,
    pub is_binary: bool,
    pub status: DiffStatus,
}

/// File change status in a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffStatus {
    Added,
    Modified,
    Deleted,
}

/// Aggregate diff summary.
#[derive(Debug, Clone, Default)]
pub struct DiffSummary {
    pub additions: u64,
    pub deletions: u64,
    pub files: usize,
}

/// Unquote git paths that may be escaped (e.g., paths with spaces or special chars).
fn unquote_git_path(path: &str) -> String {
    if path.starts_with('"') && path.ends_with('"') && path.len() >= 2 {
        let inner = &path[1..path.len() - 1];
        inner
            .replace("\\\\", "\x00")
            .replace("\\\"", "\"")
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace('\x00', "\\")
    } else {
        path.to_string()
    }
}

/// Manages shadow git snapshots for per-step undo.
///
/// Each snapshot is a git tree hash that captures the complete state
/// of the workspace at that point in time.
pub struct SnapshotManager {
    project_dir: String,
    #[allow(dead_code)]
    project_id: String,
    shadow_dir: PathBuf,
    snapshots: Vec<String>,
    initialized: bool,
}

impl SnapshotManager {
    /// Create a new snapshot manager for a project.
    pub fn new(project_dir: &str) -> Self {
        let abs_path = std::path::absolute(Path::new(project_dir))
            .unwrap_or_else(|_| PathBuf::from(project_dir));
        let project_dir_str = abs_path.to_string_lossy().to_string();
        let project_id = encode_project_id(&project_dir_str);
        let shadow_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".opendev")
            .join("snapshot")
            .join(&project_id);

        Self {
            project_dir: project_dir_str,
            project_id,
            shadow_dir,
            snapshots: Vec::new(),
            initialized: false,
        }
    }

    /// Path to the shadow .git directory.
    pub fn shadow_git_dir(&self) -> &Path {
        &self.shadow_dir
    }

    /// Number of snapshots recorded this session.
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Capture current workspace state as a tree hash.
    pub fn track(&mut self) -> Option<String> {
        if !self.ensure_initialized() {
            return None;
        }

        // Exclude OpenDev's own generated artifacts so read-only exploration does
        // not surface as user file edits in snapshot-based diff summaries.
        match self.git(&[
            "--work-tree",
            &self.project_dir,
            "add",
            "--all",
            "--force",
            "--",
            ".",
            ":(exclude).opendev/tool-output",
        ]) {
            Ok(_) => {}
            Err(e) => {
                debug!("Failed to stage files: {}", e);
                return None;
            }
        }

        match self.git(&["write-tree"]) {
            Ok(output) => {
                let tree_hash = output.trim().to_string();
                if !tree_hash.is_empty() {
                    self.snapshots.push(tree_hash.clone());
                    debug!(
                        "Snapshot captured: {} (total: {})",
                        &tree_hash[..8.min(tree_hash.len())],
                        self.snapshots.len()
                    );
                    Some(tree_hash)
                } else {
                    None
                }
            }
            Err(e) => {
                debug!("Failed to write tree: {}", e);
                None
            }
        }
    }

    /// Get list of files that changed since a snapshot.
    pub fn patch(&mut self, tree_hash: &str) -> Vec<String> {
        if !self.ensure_initialized() {
            return Vec::new();
        }

        let current_hash = match self.track() {
            Some(h) => h,
            None => return Vec::new(),
        };

        match self.git(&["diff-tree", "-r", "--name-only", tree_hash, &current_hash]) {
            Ok(output) => output
                .lines()
                .filter(|line| !line.is_empty())
                .map(|s| s.to_string())
                .collect(),
            Err(e) => {
                debug!("Failed to compute patch: {}", e);
                Vec::new()
            }
        }
    }

    /// Restore specific files (or all) from a snapshot.
    pub fn revert(&mut self, tree_hash: &str, files: Option<Vec<String>>) -> Vec<String> {
        if !self.ensure_initialized() {
            return Vec::new();
        }

        let files_to_restore = match files {
            Some(f) => f,
            None => self.patch(tree_hash),
        };

        if files_to_restore.is_empty() {
            return Vec::new();
        }

        let mut restored = Vec::new();
        for filepath in &files_to_restore {
            match self.git(&[
                "--work-tree",
                &self.project_dir,
                "checkout",
                tree_hash,
                "--",
                filepath,
            ]) {
                Ok(_) => restored.push(filepath.clone()),
                Err(_e) => {
                    debug!(
                        "Failed to restore {} from {}",
                        filepath,
                        &tree_hash[..8.min(tree_hash.len())]
                    );
                }
            }
        }

        if !restored.is_empty() {
            info!(
                "Restored {} files from snapshot {}",
                restored.len(),
                &tree_hash[..8.min(tree_hash.len())]
            );
        }
        restored
    }

    /// Full restoration to a snapshot state.
    pub fn restore(&mut self, tree_hash: &str) -> bool {
        if !self.ensure_initialized() {
            return false;
        }

        if let Err(e) = self.git(&["read-tree", tree_hash]) {
            warn!("Failed to read-tree: {}", e);
            return false;
        }

        match self.git(&[
            "--work-tree",
            &self.project_dir,
            "checkout-index",
            "--all",
            "--force",
        ]) {
            Ok(_) => {
                info!(
                    "Fully restored workspace to snapshot {}",
                    &tree_hash[..8.min(tree_hash.len())]
                );
                true
            }
            Err(e) => {
                warn!("Failed to checkout-index: {}", e);
                false
            }
        }
    }

    /// Revert to the snapshot before the most recent one.
    pub fn undo_last(&mut self) -> Option<String> {
        if self.snapshots.len() < 2 {
            return None;
        }

        self.snapshots.pop();
        let target_hash = self.snapshots.last()?.clone();

        let changed = self.patch(&target_hash);
        if changed.is_empty() {
            return None;
        }

        if self.restore(&target_hash) {
            let desc = if changed.len() <= 5 {
                format!(
                    "Reverted {} file(s) to previous state: {}",
                    changed.len(),
                    changed.join(", ")
                )
            } else {
                format!("Reverted {} file(s) to previous state", changed.len())
            };
            Some(desc)
        } else {
            None
        }
    }

    /// Compute line-level diff stats between two tree hashes.
    ///
    /// Returns a list of `(file_path, additions, deletions, status)` tuples.
    /// This is the equivalent of OpenCode's `diffNumstat()`.
    pub fn diff_numstat(&mut self, from: &str, to: &str) -> Vec<FileDiffStat> {
        if !self.ensure_initialized() {
            return Vec::new();
        }

        // Use git diff-tree with --numstat to get line counts
        match self.git(&["diff-tree", "-r", "--numstat", from, to]) {
            Ok(output) => output
                .lines()
                .filter(|line| !line.is_empty())
                .filter_map(|line| {
                    let parts: Vec<&str> = line.splitn(3, '\t').collect();
                    if parts.len() < 3 {
                        return None;
                    }
                    let additions = parts[0].parse::<u64>().unwrap_or(0);
                    let deletions = parts[1].parse::<u64>().unwrap_or(0);
                    let file_path = unquote_git_path(parts[2]);
                    // Binary files show "-" for both counts
                    let is_binary = parts[0] == "-" && parts[1] == "-";
                    Some(FileDiffStat {
                        file_path,
                        additions,
                        deletions,
                        is_binary,
                    })
                })
                .collect(),
            Err(e) => {
                debug!("Failed to compute numstat diff: {}", e);
                Vec::new()
            }
        }
    }

    /// Compute a full diff between two tree hashes, including file contents.
    ///
    /// Returns before/after content for each changed file. This is the
    /// equivalent of OpenCode's `diffFull()`.
    pub fn diff_full(&mut self, from: &str, to: &str) -> Vec<FileDiff> {
        if !self.ensure_initialized() {
            return Vec::new();
        }

        let stats = self.diff_numstat(from, to);
        let mut diffs = Vec::new();

        for stat in stats {
            let before = self.show_file(from, &stat.file_path);
            let after = self.show_file(to, &stat.file_path);

            let status = match (&before, &after) {
                (None, Some(_)) => DiffStatus::Added,
                (Some(_), None) => DiffStatus::Deleted,
                _ => DiffStatus::Modified,
            };

            diffs.push(FileDiff {
                file_path: stat.file_path,
                before: before.unwrap_or_default(),
                after: after.unwrap_or_default(),
                additions: stat.additions,
                deletions: stat.deletions,
                is_binary: stat.is_binary,
                status,
            });
        }

        diffs
    }

    /// Compute aggregate diff summary between two tree hashes.
    ///
    /// Returns total additions, deletions, and file count.
    pub fn diff_summary(&mut self, from: &str, to: &str) -> DiffSummary {
        let stats = self.diff_numstat(from, to);
        DiffSummary {
            additions: stats.iter().map(|s| s.additions).sum(),
            deletions: stats.iter().map(|s| s.deletions).sum(),
            files: stats.len(),
        }
    }

    /// Get file content at a specific tree hash.
    fn show_file(&self, tree_hash: &str, file_path: &str) -> Option<String> {
        let spec = format!("{tree_hash}:{file_path}");
        self.git(&["show", &spec]).ok()
    }

    /// Get the latest snapshot hash, if any.
    pub fn latest_snapshot(&self) -> Option<&str> {
        self.snapshots.last().map(|s| s.as_str())
    }

    /// Run git gc on the shadow repo to free space.
    pub fn cleanup(&self) {
        if !self.initialized {
            return;
        }
        let _ = self.git(&["gc", "--prune=7.days.ago", "--quiet"]);
    }

    fn ensure_initialized(&mut self) -> bool {
        if self.initialized {
            return true;
        }

        if let Err(e) = std::fs::create_dir_all(&self.shadow_dir) {
            warn!("Failed to create shadow dir: {}", e);
            return false;
        }

        // Check if already a git repo
        if self.shadow_dir.join("HEAD").exists() {
            self.initialized = true;
            return true;
        }

        // Initialize bare-ish shadow repo
        match self.git(&["init", "--bare"]) {
            Ok(_) => {
                self.initialized = true;
                info!(
                    "Shadow snapshot repo initialized at {}",
                    self.shadow_dir.display()
                );
                true
            }
            Err(e) => {
                warn!("Failed to initialize shadow snapshot repo: {}", e);
                false
            }
        }
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        let mut cmd = Command::new("git");
        cmd.arg("--git-dir")
            .arg(self.shadow_dir.to_string_lossy().as_ref());
        for arg in args {
            cmd.arg(arg);
        }
        cmd.current_dir(&self.project_dir);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(stderr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_project_id() {
        let id1 = encode_project_id("/Users/foo/project");
        let id2 = encode_project_id("/Users/foo/project");
        assert_eq!(id1, id2); // Deterministic

        let id3 = encode_project_id("/Users/bar/project");
        assert_ne!(id1, id3); // Different paths -> different IDs

        assert_eq!(id1.len(), 16); // Fixed width hex
    }

    #[test]
    fn test_snapshot_manager_new() {
        let mgr = SnapshotManager::new("/tmp/test-project");
        assert_eq!(mgr.snapshot_count(), 0);
        assert!(!mgr.initialized);
    }

    // Integration tests that require git are skipped in CI
    // but can be run locally with: cargo test -- --ignored

    #[test]
    fn test_unquote_git_path_plain() {
        assert_eq!(unquote_git_path("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn test_unquote_git_path_quoted() {
        assert_eq!(
            unquote_git_path("\"path with spaces/file.rs\""),
            "path with spaces/file.rs"
        );
    }

    #[test]
    fn test_unquote_git_path_escaped() {
        assert_eq!(
            unquote_git_path("\"path\\\\with\\\\backslashes\""),
            "path\\with\\backslashes"
        );
    }

    #[test]
    fn test_diff_summary_default() {
        let summary = DiffSummary::default();
        assert_eq!(summary.additions, 0);
        assert_eq!(summary.deletions, 0);
        assert_eq!(summary.files, 0);
    }

    #[test]
    fn test_diff_status_equality() {
        assert_eq!(DiffStatus::Added, DiffStatus::Added);
        assert_ne!(DiffStatus::Added, DiffStatus::Modified);
        assert_ne!(DiffStatus::Modified, DiffStatus::Deleted);
    }

    #[test]
    fn test_latest_snapshot_empty() {
        let mgr = SnapshotManager::new("/tmp/test-project");
        assert!(mgr.latest_snapshot().is_none());
    }

    #[test]
    #[ignore]
    fn test_snapshot_diff_numstat() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project_dir = tmp.path().to_string_lossy().to_string();

        Command::new("git")
            .args(["init"])
            .current_dir(&project_dir)
            .output()
            .unwrap();

        // Create initial file
        std::fs::write(tmp.path().join("test.txt"), "line1\nline2\nline3\n").unwrap();
        let mut mgr = SnapshotManager::new(&project_dir);
        let hash1 = mgr.track().unwrap();

        // Modify file — add 2 lines, remove 1
        std::fs::write(
            tmp.path().join("test.txt"),
            "line1\nline2_modified\nline3\nnew_line4\nnew_line5\n",
        )
        .unwrap();
        let hash2 = mgr.track().unwrap();

        let stats = mgr.diff_numstat(&hash1, &hash2);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].file_path, "test.txt");
        assert!(stats[0].additions > 0);
        assert!(!stats[0].is_binary);
    }

    #[test]
    #[ignore]
    fn test_snapshot_diff_full() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project_dir = tmp.path().to_string_lossy().to_string();

        Command::new("git")
            .args(["init"])
            .current_dir(&project_dir)
            .output()
            .unwrap();

        std::fs::write(tmp.path().join("hello.rs"), "fn main() {}\n").unwrap();
        let mut mgr = SnapshotManager::new(&project_dir);
        let hash1 = mgr.track().unwrap();

        std::fs::write(
            tmp.path().join("hello.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
        std::fs::write(tmp.path().join("new_file.txt"), "new content\n").unwrap();
        let hash2 = mgr.track().unwrap();

        let diffs = mgr.diff_full(&hash1, &hash2);
        assert!(diffs.len() >= 2);

        let hello_diff = diffs.iter().find(|d| d.file_path == "hello.rs").unwrap();
        assert_eq!(hello_diff.status, DiffStatus::Modified);
        assert!(hello_diff.before.contains("fn main()"));
        assert!(hello_diff.after.contains("println!"));

        let new_diff = diffs
            .iter()
            .find(|d| d.file_path == "new_file.txt")
            .unwrap();
        assert_eq!(new_diff.status, DiffStatus::Added);
        assert!(new_diff.before.is_empty());
        assert!(new_diff.after.contains("new content"));
    }

    #[test]
    #[ignore]
    fn test_snapshot_diff_summary() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project_dir = tmp.path().to_string_lossy().to_string();

        Command::new("git")
            .args(["init"])
            .current_dir(&project_dir)
            .output()
            .unwrap();

        std::fs::write(tmp.path().join("a.txt"), "line1\n").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "line1\n").unwrap();
        let mut mgr = SnapshotManager::new(&project_dir);
        let hash1 = mgr.track().unwrap();

        std::fs::write(tmp.path().join("a.txt"), "line1\nline2\n").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "modified\n").unwrap();
        let hash2 = mgr.track().unwrap();

        let summary = mgr.diff_summary(&hash1, &hash2);
        assert_eq!(summary.files, 2);
        assert!(summary.additions > 0);
    }

    #[test]
    #[ignore]
    fn test_snapshot_track_and_patch() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project_dir = tmp.path().to_string_lossy().to_string();

        // Initialize a git repo in the project dir
        Command::new("git")
            .args(["init"])
            .current_dir(&project_dir)
            .output()
            .unwrap();

        // Create a file
        std::fs::write(tmp.path().join("test.txt"), "hello").unwrap();

        let mut mgr = SnapshotManager::new(&project_dir);
        let hash1 = mgr.track();
        assert!(hash1.is_some());
        assert_eq!(mgr.snapshot_count(), 1);

        // Modify the file
        std::fs::write(tmp.path().join("test.txt"), "hello world").unwrap();

        let changed = mgr.patch(hash1.as_ref().unwrap());
        assert!(changed.contains(&"test.txt".to_string()));
    }
}
