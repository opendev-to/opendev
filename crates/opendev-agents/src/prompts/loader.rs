//! Utilities for loading system prompts from configuration files.
//!
//! Mirrors `opendev/core/agents/prompts/loader.py`.
//!
//! Supports three loading strategies (in priority order):
//! 1. Embedded templates via `include_str!` at compile time
//! 2. Runtime loading from user-specified template directories
//! 3. Fallback text supplied by the caller

use std::path::{Path, PathBuf};

use super::composer::strip_frontmatter;
use super::embedded;

/// Prompt loader that resolves template files.
///
/// Resolution order for each prompt:
/// 1. Embedded store (compile-time, zero I/O)
/// 2. Filesystem `templates_dir` (runtime, user overrides)
#[derive(Debug, Clone)]
pub struct PromptLoader {
    templates_dir: PathBuf,
}

impl PromptLoader {
    /// Create a new loader rooted at the given templates directory.
    pub fn new(templates_dir: impl Into<PathBuf>) -> Self {
        Self {
            templates_dir: templates_dir.into(),
        }
    }

    /// Get the path to a prompt file.
    ///
    /// Prefers `.md` format, falls back to `.txt` for backward compatibility.
    pub fn get_prompt_path(&self, prompt_name: &str) -> PathBuf {
        let md_path = self.templates_dir.join(format!("{prompt_name}.md"));
        if md_path.exists() {
            return md_path;
        }
        self.templates_dir.join(format!("{prompt_name}.txt"))
    }

    /// Load a system prompt from file.
    ///
    /// Strips YAML frontmatter from `.md` files.
    /// Tries embedded store first, then filesystem.
    pub fn load_prompt(&self, prompt_name: &str) -> Result<String, PromptLoadError> {
        self.load_prompt_with_fallback(prompt_name, None)
    }

    /// Load a system prompt with an optional fallback.
    pub fn load_prompt_with_fallback(
        &self,
        prompt_name: &str,
        fallback: Option<&str>,
    ) -> Result<String, PromptLoadError> {
        // 1. Try filesystem first (user overrides take priority)
        let prompt_file = self.get_prompt_path(prompt_name);
        if prompt_file.exists() {
            let content = std::fs::read_to_string(&prompt_file)
                .map_err(|e| PromptLoadError::Io(prompt_file.clone(), e))?;

            // Strip frontmatter for .md files
            return if prompt_file.extension().is_some_and(|ext| ext == "md") {
                Ok(strip_frontmatter(&content))
            } else {
                Ok(content.trim().to_string())
            };
        }

        // 2. Try embedded (.md key)
        let md_key = format!("{prompt_name}.md");
        if let Some(raw) = embedded::get_embedded(&md_key) {
            return Ok(strip_frontmatter(raw));
        }

        // 3. Fallback
        match fallback {
            Some(fb) => Ok(fb.to_string()),
            None => Err(PromptLoadError::NotFound(prompt_file)),
        }
    }

    /// Load a tool description from its markdown template.
    pub fn load_tool_description(&self, tool_name: &str) -> Result<String, PromptLoadError> {
        let kebab_name = tool_name.replace('_', "-");
        self.load_prompt(&format!("tools/tool-{kebab_name}"))
    }

    /// Get the templates directory.
    pub fn templates_dir(&self) -> &Path {
        &self.templates_dir
    }
}

/// Errors that can occur when loading prompts.
#[derive(Debug, thiserror::Error)]
pub enum PromptLoadError {
    #[error("Prompt file not found: {0}")]
    NotFound(PathBuf),

    #[error("Failed to read prompt file {0}: {1}")]
    Io(PathBuf, std::io::Error),
}

#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;
