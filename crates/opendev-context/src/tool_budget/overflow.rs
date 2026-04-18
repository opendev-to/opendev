//! On-disk store for oversized tool results.
//!
//! When a tool result exceeds its budget, the full content is written
//! to a unique file under the project's overflow directory and the
//! displayed message references it by relative path.

use std::path::{Path, PathBuf};

use chrono::Local;

use crate::compaction::TOOL_RESULT_BUDGET_OVERFLOW_DIR;

/// Writes overflow tool result content to disk under a project-scoped
/// directory. Reference paths returned to the LLM are relative to the
/// project root so they read naturally in transcripts.
#[derive(Debug, Clone)]
pub struct OverflowStore {
    /// Absolute path to the directory where overflow files are written.
    overflow_dir: PathBuf,
    /// Project root used to compute relative reference paths.
    project_root: PathBuf,
}

impl OverflowStore {
    /// Construct a store rooted at `project_root`. Overflow files are
    /// written under `project_root/.opendev/tool-results/`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let project_root = project_root.into();
        let overflow_dir = project_root
            .join(".opendev")
            .join(TOOL_RESULT_BUDGET_OVERFLOW_DIR);
        Self {
            overflow_dir,
            project_root,
        }
    }

    /// Construct a store with an explicit overflow directory. Useful in
    /// tests where the project root and the overflow dir are decoupled.
    pub fn with_dir(project_root: impl Into<PathBuf>, overflow_dir: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            overflow_dir: overflow_dir.into(),
        }
    }

    /// Persist `content` to a unique file. Returns a path suitable for
    /// inclusion in the displayed tool message — relative to the project
    /// root when possible, absolute otherwise.
    ///
    /// On I/O failure returns `None`; callers should fall back to a
    /// truncation-only display so a disk error never breaks the agent
    /// loop.
    pub fn write(&self, tool_name: &str, tool_call_id: &str, content: &str) -> Option<String> {
        if let Err(err) = std::fs::create_dir_all(&self.overflow_dir) {
            tracing::warn!(
                error = %err,
                dir = %self.overflow_dir.display(),
                "tool-result overflow: failed to create directory",
            );
            return None;
        }

        let stamp = Local::now().format("%Y-%m-%dT%H-%M-%S");
        let safe_tool = sanitize_for_filename(tool_name);
        let safe_id = sanitize_for_filename(tool_call_id);
        let filename = format!("{stamp}-{safe_tool}-{safe_id}.txt");
        let abs_path = self.overflow_dir.join(filename);

        if let Err(err) = std::fs::write(&abs_path, content) {
            tracing::warn!(
                error = %err,
                path = %abs_path.display(),
                "tool-result overflow: failed to write file",
            );
            return None;
        }

        Some(self.display_path(&abs_path))
    }

    /// Convert an absolute overflow path to the form shown to the LLM.
    fn display_path(&self, abs_path: &Path) -> String {
        abs_path
            .strip_prefix(&self.project_root)
            .map(|rel| rel.to_string_lossy().into_owned())
            .unwrap_or_else(|_| abs_path.to_string_lossy().into_owned())
    }
}

/// Replace filesystem-unsafe characters with `_`. Keeps filenames short
/// and predictable across platforms without depending on extra crates.
fn sanitize_for_filename(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars().take(64) {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("unnamed");
    }
    out
}
