//! Git worktree manager — create and manage isolated workspaces.
//!
//! Uses `git worktree` to create lightweight, isolated copies of the repository
//! for subagent work. Each worktree gets its own branch and can be independently
//! modified without affecting the main workspace.

use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::{debug, warn};

/// Information about a git worktree.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// Absolute path to the worktree directory.
    pub path: PathBuf,
    /// Branch checked out in this worktree.
    pub branch: String,
    /// HEAD commit hash.
    pub head: String,
    /// Whether this is the main worktree (bare = false).
    pub is_main: bool,
}

/// Manages git worktrees for workspace isolation.
pub struct WorktreeManager {
    /// Root of the main git repository.
    repo_root: PathBuf,
}

impl WorktreeManager {
    /// Create a new worktree manager for the given repository root.
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
        }
    }

    /// Create a new worktree with an auto-generated branch name.
    ///
    /// Returns the worktree path and branch name.
    pub fn create(&self, prefix: &str) -> Result<WorktreeInfo, String> {
        let branch_name = format!("opendev/{prefix}/{}", generate_short_id());
        let worktree_path = self.worktrees_dir().join(branch_name.replace('/', "_"));

        self.create_at(&worktree_path, &branch_name)
    }

    /// Create a worktree at a specific path with a specific branch.
    pub fn create_at(&self, path: &Path, branch: &str) -> Result<WorktreeInfo, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dir: {e}"))?;
        }

        let output = Command::new("git")
            .args(["worktree", "add", &path.to_string_lossy(), "-b", branch])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| format!("Failed to run git: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git worktree add failed: {stderr}"));
        }

        debug!(
            "Created worktree at {} on branch {}",
            path.display(),
            branch
        );

        // Get HEAD
        let head = self
            .git_in(path, &["rev-parse", "HEAD"])
            .unwrap_or_else(|| "unknown".to_string());

        Ok(WorktreeInfo {
            path: path.to_path_buf(),
            branch: branch.to_string(),
            head: head.trim().to_string(),
            is_main: false,
        })
    }

    /// List all worktrees.
    pub fn list(&self) -> Vec<WorktreeInfo> {
        let output = match Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.repo_root)
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                warn!("Failed to list worktrees: {e}");
                return Vec::new();
            }
        };

        if !output.status.success() {
            return Vec::new();
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_porcelain_output(&stdout)
    }

    /// Remove a worktree by path.
    pub fn remove(&self, path: &Path) -> Result<(), String> {
        let output = Command::new("git")
            .args(["worktree", "remove", &path.to_string_lossy(), "--force"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| format!("Failed to run git: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git worktree remove failed: {stderr}"));
        }

        debug!("Removed worktree at {}", path.display());
        Ok(())
    }

    /// Remove all stale/pruned worktrees.
    pub fn prune(&self) -> Result<(), String> {
        let output = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| format!("Failed to run git: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git worktree prune failed: {stderr}"));
        }

        Ok(())
    }

    /// Clean up all opendev-created worktrees.
    pub fn cleanup_all(&self) -> Vec<String> {
        let worktrees = self.list();
        let mut removed = Vec::new();

        for wt in worktrees {
            if wt.is_main {
                continue;
            }
            if wt.branch.starts_with("opendev/") {
                match self.remove(&wt.path) {
                    Ok(()) => removed.push(wt.path.to_string_lossy().to_string()),
                    Err(e) => warn!("Failed to remove worktree {}: {}", wt.path.display(), e),
                }
            }
        }

        let _ = self.prune();
        removed
    }

    fn worktrees_dir(&self) -> PathBuf {
        self.repo_root.join(".git").join("opendev-worktrees")
    }

    fn git_in(&self, dir: &Path, args: &[&str]) -> Option<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            None
        }
    }
}

/// Parse `git worktree list --porcelain` output.
fn parse_porcelain_output(output: &str) -> Vec<WorktreeInfo> {
    let mut worktrees = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut head = String::new();
    let mut branch = String::new();
    let mut is_main = true;

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            // Save previous entry
            if let Some(p) = path.take() {
                worktrees.push(WorktreeInfo {
                    path: p,
                    branch: std::mem::take(&mut branch),
                    head: std::mem::take(&mut head),
                    is_main,
                });
            }
            path = Some(PathBuf::from(rest));
            is_main = worktrees.is_empty(); // First worktree is main
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            head = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("branch ") {
            // refs/heads/branch-name → branch-name
            branch = rest.strip_prefix("refs/heads/").unwrap_or(rest).to_string();
        }
    }

    // Push last entry
    if let Some(p) = path {
        worktrees.push(WorktreeInfo {
            path: p,
            branch,
            head,
            is_main,
        });
    }

    worktrees
}

fn generate_short_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let mut hasher = DefaultHasher::new();
    SystemTime::now().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    format!("{:08x}", hasher.finish() as u32)
}

impl std::fmt::Debug for WorktreeManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorktreeManager")
            .field("repo_root", &self.repo_root)
            .finish()
    }
}

#[cfg(test)]
#[path = "worktree_tests.rs"]
mod tests;
