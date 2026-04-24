//! Skills system for lazy-loaded knowledge modules.
//!
//! Skills are markdown files with YAML frontmatter that inject knowledge and
//! instructions into the main agent context on demand. Unlike subagents
//! (separate sessions), skills extend the current conversation's capabilities.
//!
//! ## Directory Structure
//! Skills are loaded from (in priority order):
//! - `<project>/.opendev/skills/` (project local, highest priority)
//! - `~/.opendev/skills/` (user global)
//! - Built-in skills embedded in the binary
//!
//! ## Skill File Format
//! ```markdown
//! ---
//! name: commit
//! description: Git commit best practices
//! namespace: default
//! ---
//!
//! # Git Commit Skill
//! When making commits: ...
//! ```

mod builtins;
mod discovery;
mod loader;
mod metadata;
mod parsing;

pub use loader::SkillLoader;
pub use metadata::{
    CompanionFile, LoadedSkill, SkillContext, SkillEffort, SkillHookDef, SkillMetadata, SkillSource,
};
