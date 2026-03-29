//! Skill metadata and loaded skill types.

use std::path::PathBuf;
use std::time::SystemTime;

/// Where a skill was loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    /// Compiled into the binary.
    Builtin,
    /// From `~/.opendev/skills/`.
    UserGlobal,
    /// From `<project>/.opendev/skills/`.
    Project,
    /// Downloaded from a remote URL.
    Url(String),
}

impl std::fmt::Display for SkillSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillSource::Builtin => write!(f, "builtin"),
            SkillSource::UserGlobal => write!(f, "user-global"),
            SkillSource::Project => write!(f, "project"),
            SkillSource::Url(url) => write!(f, "url:{url}"),
        }
    }
}

/// Metadata extracted from a skill file's YAML frontmatter.
#[derive(Debug, Clone)]
pub struct SkillMetadata {
    /// Skill name (e.g. `"commit"`).
    pub name: String,
    /// Human-readable description, ideally starting with "Use when...".
    pub description: String,
    /// Namespace for grouping (default: `"default"`).
    pub namespace: String,
    /// Path to the source `.md` file on disk (None for builtins).
    pub path: Option<PathBuf>,
    /// Where this skill was discovered.
    pub source: SkillSource,
    /// Optional model override for this skill (e.g. `"gpt-4o"`, `"claude-sonnet-4-5-20250514"`).
    /// When set, the agent should use this model instead of the default when executing the skill.
    pub model: Option<String>,
    /// Optional agent override for this skill.
    /// When set, the skill should be executed by the specified agent instead of the current one.
    pub agent: Option<String>,
}

impl SkillMetadata {
    /// Build the full name including namespace prefix.
    ///
    /// Returns `"name"` for default namespace, `"namespace:name"` otherwise.
    pub fn full_name(&self) -> String {
        if self.namespace == "default" {
            self.name.clone()
        } else {
            format!("{}:{}", self.namespace, self.name)
        }
    }

    /// Estimate token count for the skill file.
    ///
    /// Uses a rough heuristic of ~4 characters per token.
    pub fn estimate_tokens(&self) -> Option<usize> {
        if let Some(path) = &self.path
            && let Ok(content) = std::fs::read_to_string(path)
        {
            return Some(content.len() / 4);
        }
        None
    }
}

/// A companion file discovered alongside a directory-style skill.
#[derive(Debug, Clone)]
pub struct CompanionFile {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Path relative to the skill directory.
    pub relative_path: String,
}

/// A fully loaded skill with its content ready for injection.
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    /// Metadata from the frontmatter.
    pub metadata: SkillMetadata,
    /// The markdown body content (frontmatter stripped).
    pub content: String,
    /// Companion files found alongside the skill (for directory-style skills).
    pub companion_files: Vec<CompanionFile>,
    /// File modification time when the skill was cached.
    /// Used for cache invalidation: if the file's mtime is newer, reload.
    pub cached_mtime: Option<SystemTime>,
}

impl LoadedSkill {
    /// Estimate the token count of the loaded content.
    pub fn estimate_tokens(&self) -> usize {
        self.content.len() / 4
    }
}

#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;
