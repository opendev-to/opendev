//! Lazy per-subdirectory instruction injection.
//!
//! When the agent reads a file, this module checks parent directories for
//! instruction files (AGENTS.md, CLAUDE.md) that haven't been injected yet
//! and returns their content for injection into the conversation.
//!
//! Mirrors OpenCode's `InstructionPrompt.resolve()` behavior.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tracing::debug;

/// Recognized instruction file names (same order as environment.rs).
const INSTRUCTION_FILENAMES: &[&str] = &["AGENTS.md", "CLAUDE.md", "CONTEXT.md"];

/// Additional instruction files from other AI tools.
const COMPAT_INSTRUCTION_FILES: &[&str] = &[".cursorrules", ".github/copilot-instructions.md"];

/// Maximum instruction file size to inject (50 KB).
const MAX_INSTRUCTION_SIZE: usize = 50 * 1024;

/// Tracks which subdirectory instruction files have been injected into the
/// conversation, and discovers new ones when files are read.
#[derive(Debug, Clone)]
pub struct SubdirInstructionTracker {
    /// Canonical paths of instruction files already injected (at startup or
    /// during the session).
    injected: HashSet<PathBuf>,
    /// The project root (git root or working dir). We don't walk above this.
    project_root: PathBuf,
}

/// An instruction file discovered from a subdirectory.
#[derive(Debug, Clone)]
pub struct SubdirInstruction {
    /// Path to the instruction file.
    pub path: PathBuf,
    /// Relative path from project root for display.
    pub relative_path: String,
    /// File contents.
    pub content: String,
}

impl SubdirInstructionTracker {
    /// Create a new tracker, pre-populating with instruction files already
    /// injected at startup (from the system prompt).
    pub fn new(project_root: PathBuf, startup_files: &[PathBuf]) -> Self {
        let mut injected = HashSet::new();
        for path in startup_files {
            if let Ok(canonical) = path.canonicalize() {
                injected.insert(canonical);
            }
        }
        Self {
            injected,
            project_root,
        }
    }

    /// Check if a file path triggers any new subdirectory instruction injection.
    ///
    /// Walks from the directory containing `file_path` up toward the project root,
    /// looking for AGENTS.md / CLAUDE.md files that haven't been injected yet.
    /// Returns any new instruction files found (and marks them as injected).
    pub fn check_file_read(&mut self, file_path: &Path) -> Vec<SubdirInstruction> {
        let dir = if file_path.is_dir() {
            file_path.to_path_buf()
        } else {
            match file_path.parent() {
                Some(p) => p.to_path_buf(),
                None => return Vec::new(),
            }
        };

        let canonical_root = self
            .project_root
            .canonicalize()
            .unwrap_or_else(|_| self.project_root.clone());
        let mut results = Vec::new();
        let mut current = dir;

        loop {
            // Check each instruction filename in this directory
            for filename in INSTRUCTION_FILENAMES {
                let candidate = current.join(filename);
                if let Ok(canonical) = candidate.canonicalize() {
                    if self.injected.contains(&canonical) {
                        continue; // Already injected
                    }

                    // Read the file
                    if let Ok(content) = std::fs::read_to_string(&canonical) {
                        let content = if content.len() > MAX_INSTRUCTION_SIZE {
                            content[..MAX_INSTRUCTION_SIZE].to_string()
                        } else {
                            content
                        };

                        let relative = canonical
                            .strip_prefix(&canonical_root)
                            .unwrap_or(&canonical)
                            .display()
                            .to_string();

                        debug!(path = %relative, "Injecting subdirectory instruction file");

                        self.injected.insert(canonical.clone());
                        results.push(SubdirInstruction {
                            path: canonical,
                            relative_path: relative,
                            content,
                        });
                    }
                }
            }

            // Also check .opendev/instructions.md
            for subdir in &[".opendev"] {
                let candidate = current.join(subdir).join("instructions.md");
                if let Ok(canonical) = candidate.canonicalize() {
                    if self.injected.contains(&canonical) {
                        continue;
                    }
                    if let Ok(content) = std::fs::read_to_string(&canonical) {
                        let content = if content.len() > MAX_INSTRUCTION_SIZE {
                            content[..MAX_INSTRUCTION_SIZE].to_string()
                        } else {
                            content
                        };
                        let relative = canonical
                            .strip_prefix(&canonical_root)
                            .unwrap_or(&canonical)
                            .display()
                            .to_string();
                        debug!(path = %relative, "Injecting subdirectory instruction file");
                        self.injected.insert(canonical.clone());
                        results.push(SubdirInstruction {
                            path: canonical,
                            relative_path: relative,
                            content,
                        });
                    }
                }
            }

            // Check compatibility instruction files (.cursorrules, copilot, etc.)
            for compat_path in COMPAT_INSTRUCTION_FILES {
                let candidate = current.join(compat_path);
                if let Ok(canonical) = candidate.canonicalize() {
                    if self.injected.contains(&canonical) {
                        continue;
                    }
                    if let Ok(content) = std::fs::read_to_string(&canonical) {
                        let content = if content.len() > MAX_INSTRUCTION_SIZE {
                            content[..MAX_INSTRUCTION_SIZE].to_string()
                        } else {
                            content
                        };
                        let relative = canonical
                            .strip_prefix(&canonical_root)
                            .unwrap_or(&canonical)
                            .display()
                            .to_string();
                        debug!(path = %relative, "Injecting compatibility instruction file");
                        self.injected.insert(canonical.clone());
                        results.push(SubdirInstruction {
                            path: canonical,
                            relative_path: relative,
                            content,
                        });
                    }
                }
            }

            // Stop at project root
            let canonical_current = current.canonicalize().unwrap_or_else(|_| current.clone());
            if canonical_current == canonical_root {
                break;
            }

            // Move up
            if !current.pop() {
                break;
            }
        }

        results
    }

    /// After compaction removes middle messages, allow subdirectory instructions
    /// to be re-discovered on the next file read.
    ///
    /// Preserves startup files (root-level instructions in system prompt) and
    /// any instructions whose content is still present in the remaining messages.
    pub fn reset_after_compaction(
        &mut self,
        startup_files: &[PathBuf],
        remaining_messages: &[serde_json::Value],
    ) {
        // Collect paths of instructions still present in remaining messages
        let mut still_present = HashSet::new();
        for msg in remaining_messages {
            if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                for path in &self.injected {
                    let path_str = path.display().to_string();
                    if content.contains(&path_str)
                        || content
                            .contains(path.file_name().unwrap_or_default().to_str().unwrap_or(""))
                    {
                        still_present.insert(path.clone());
                    }
                }
            }
        }

        self.injected = still_present;

        // Always keep startup files marked as injected (they live in system prompt)
        for path in startup_files {
            if let Ok(canonical) = path.canonicalize() {
                self.injected.insert(canonical);
            }
        }
    }

    /// Return the number of instruction files currently tracked.
    pub fn injected_count(&self) -> usize {
        self.injected.len()
    }
}

#[cfg(test)]
#[path = "subdir_instructions_tests.rs"]
mod tests;
