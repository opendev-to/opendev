//! Git worktree management for isolated agent workspaces.
//!
//! Mirrors `opendev/core/git/worktree.py`.
//!
//! Provides [`WorktreeManager`] for creating, listing, removing, and
//! cleaning up git worktrees.  Each worktree gives an agent an isolated
//! checkout where it can make changes without interfering with other
//! sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use thiserror::Error;
use tokio::process::Command;
use tracing::{debug, warn};

// ── Naming ──────────────────────────────────────────────────────────────────

const ADJECTIVES: &[&str] = &[
    "swift", "bright", "calm", "bold", "keen", "warm", "cool", "deep", "fair", "fine", "glad",
    "pure", "safe", "wise", "neat",
];

const NOUNS: &[&str] = &[
    "branch", "patch", "spike", "draft", "build", "probe", "trial", "craft", "forge", "bloom",
    "spark", "quest", "grove", "ridge", "haven",
];

/// Generate a random adjective-noun worktree name.
fn random_name() -> String {
    use std::time::SystemTime;
    // Simple deterministic-enough RNG from timestamp nanos
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize;
    let adj = ADJECTIVES[seed % ADJECTIVES.len()];
    let noun = NOUNS[(seed / ADJECTIVES.len()) % NOUNS.len()];
    format!("{adj}-{noun}")
}

// ── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("git command failed: {0}")]
    GitError(String),
    #[error("worktree not found: {0}")]
    NotFound(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("worktree already exists: {0}")]
    AlreadyExists(String),
}

// ── WorktreeInfo ────────────────────────────────────────────────────────────

/// Information about a single git worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    /// Absolute path to the worktree directory.
    pub path: String,
    /// Branch checked out in this worktree.
    pub branch: String,
    /// HEAD commit hash.
    pub commit: String,
    /// Whether this is the main (bare) worktree.
    pub is_main: bool,
}

impl std::fmt::Display for WorktreeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let suffix = if self.is_main { " (main)" } else { "" };
        write!(f, "Worktree({}{}, {})", self.branch, suffix, self.path)
    }
}

// ── WorktreeManager ─────────────────────────────────────────────────────────

/// Manages git worktrees for a project.
pub struct WorktreeManager {
    /// Root directory of the git repository.
    project_dir: PathBuf,
    /// Base directory for storing worktrees.
    worktree_base: PathBuf,
    /// Tracked worktrees in session state: name -> WorktreeInfo.
    tracked: HashMap<String, WorktreeInfo>,
}

impl WorktreeManager {
    /// Create a new manager for the given project directory.
    ///
    /// Worktrees are stored under `~/.opendev/data/worktree/`.
    pub fn new(project_dir: impl Into<PathBuf>) -> Self {
        let project_dir = project_dir.into();
        let worktree_base = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".opendev")
            .join("data")
            .join("worktree");
        Self {
            project_dir,
            worktree_base,
            tracked: HashMap::new(),
        }
    }

    /// Create a new manager with a custom worktree base directory.
    ///
    /// Primarily useful for tests.
    pub fn with_base(project_dir: impl Into<PathBuf>, worktree_base: impl Into<PathBuf>) -> Self {
        Self {
            project_dir: project_dir.into(),
            worktree_base: worktree_base.into(),
            tracked: HashMap::new(),
        }
    }

    /// Get the project directory.
    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }

    /// Get the worktree base directory.
    pub fn worktree_base(&self) -> &Path {
        &self.worktree_base
    }

    /// Create a new worktree.
    ///
    /// - `name`: worktree name (auto-generated if `None`)
    /// - `branch`: branch name (defaults to `worktree-{name}`)
    /// - `base_branch`: base commit/branch to start from (defaults to `"HEAD"`)
    pub async fn create(
        &mut self,
        name: Option<&str>,
        branch: Option<&str>,
        base_branch: &str,
    ) -> Result<WorktreeInfo, WorktreeError> {
        let name = name.map(String::from).unwrap_or_else(random_name);
        let branch = branch
            .map(String::from)
            .unwrap_or_else(|| format!("worktree-{name}"));
        let worktree_path = self.worktree_base.join(&name);

        if worktree_path.exists() {
            return Err(WorktreeError::AlreadyExists(name));
        }

        // Ensure parent exists
        if let Some(parent) = worktree_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let output = Command::new("git")
            .args(["worktree", "add", "-b", &branch])
            .arg(&worktree_path)
            .arg(base_branch)
            .current_dir(&self.project_dir)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            warn!("Failed to create worktree: {stderr}");
            return Err(WorktreeError::GitError(stderr));
        }

        // Read HEAD commit in the new worktree
        let commit = self
            .git_output(&["rev-parse", "HEAD"], Some(&worktree_path))
            .await
            .unwrap_or_default();

        let info = WorktreeInfo {
            path: worktree_path.to_string_lossy().to_string(),
            branch,
            commit,
            is_main: false,
        };

        debug!("Created worktree: {info}");
        self.tracked.insert(name, info.clone());
        Ok(info)
    }

    /// List all worktrees for the project (from `git worktree list --porcelain`).
    pub async fn list(&self) -> Result<Vec<WorktreeInfo>, WorktreeError> {
        let raw = self
            .git_output(&["worktree", "list", "--porcelain"], None)
            .await
            .ok_or_else(|| WorktreeError::GitError("git worktree list failed".into()))?;

        Ok(parse_porcelain_output(&raw))
    }

    /// Remove a worktree by name (or absolute path).
    pub async fn remove(&mut self, name: &str, force: bool) -> Result<(), WorktreeError> {
        let worktree_path = self.resolve_worktree_path(name);

        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        let path_str = worktree_path.to_string_lossy().to_string();
        args.push(&path_str);

        let output = Command::new("git")
            .args(&args)
            .current_dir(&self.project_dir)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            warn!("Failed to remove worktree: {stderr}");
            return Err(WorktreeError::GitError(stderr));
        }

        self.tracked.remove(name);
        debug!("Removed worktree: {name}");
        Ok(())
    }

    /// Clean up stale/prunable worktree references.
    pub async fn cleanup(&self) -> Result<String, WorktreeError> {
        let output = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.project_dir)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(WorktreeError::GitError(stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        debug!("Worktree cleanup done");
        Ok(stdout)
    }

    /// Get tracked worktrees in session state.
    pub fn tracked(&self) -> &HashMap<String, WorktreeInfo> {
        &self.tracked
    }

    /// Track a worktree in session state.
    pub fn track(&mut self, name: String, info: WorktreeInfo) {
        self.tracked.insert(name, info);
    }

    /// Untrack a worktree from session state.
    pub fn untrack(&mut self, name: &str) -> Option<WorktreeInfo> {
        self.tracked.remove(name)
    }

    // ── internal helpers ────────────────────────────────────────────────────

    fn resolve_worktree_path(&self, name: &str) -> PathBuf {
        let candidate = self.worktree_base.join(name);
        if candidate.exists() {
            candidate
        } else {
            // Try treating as absolute path
            PathBuf::from(name)
        }
    }

    async fn git_output(&self, args: &[&str], cwd: Option<&Path>) -> Option<String> {
        let cwd = cwd.unwrap_or(&self.project_dir);
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .await
            .ok()?;

        if !output.status.success() {
            return None;
        }

        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

// ── Parsing ─────────────────────────────────────────────────────────────────

/// Parse `git worktree list --porcelain` output into [`WorktreeInfo`] entries.
fn parse_porcelain_output(raw: &str) -> Vec<WorktreeInfo> {
    let mut worktrees = Vec::new();
    let mut path = String::new();
    let mut commit = String::new();
    let mut branch = String::new();
    let mut is_main = false;
    let mut has_entry = false;

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            // Flush previous entry
            if has_entry {
                worktrees.push(WorktreeInfo {
                    path: std::mem::take(&mut path),
                    branch: if branch.is_empty() {
                        "detached".to_string()
                    } else {
                        std::mem::take(&mut branch)
                    },
                    commit: std::mem::take(&mut commit),
                    is_main,
                });
                is_main = false;
            }
            path = rest.to_string();
            has_entry = true;
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            commit = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("branch ") {
            branch = rest.replace("refs/heads/", "");
        } else if line == "bare" {
            is_main = true;
        }
    }

    // Flush last entry
    if has_entry {
        worktrees.push(WorktreeInfo {
            path,
            branch: if branch.is_empty() {
                "detached".to_string()
            } else {
                branch
            },
            commit,
            is_main,
        });
    }

    worktrees
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "worktree_tests.rs"]
mod tests;
