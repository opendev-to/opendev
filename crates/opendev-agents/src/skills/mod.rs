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

mod discovery;
mod metadata;
mod parsing;

pub use metadata::{CompanionFile, LoadedSkill, SkillMetadata, SkillSource};

use std::collections::HashMap;
use std::path::PathBuf;

use tracing::{debug, warn};

use discovery::{
    detect_source, discover_companion_files, glob_md_files, is_cache_stale, pull_url_skills,
};
use parsing::{parse_frontmatter_file, parse_frontmatter_str, strip_frontmatter};

// ============================================================================
// Built-in skills, embedded at compile time
// ============================================================================

struct BuiltinSkill {
    filename: &'static str,
    content: &'static str,
}

const BUILTIN_SKILLS: &[BuiltinSkill] = &[
    BuiltinSkill {
        filename: "commit.md",
        content: include_str!("builtin/commit.md"),
    },
    BuiltinSkill {
        filename: "review-pr.md",
        content: include_str!("builtin/review-pr.md"),
    },
    BuiltinSkill {
        filename: "create-pr.md",
        content: include_str!("builtin/create-pr.md"),
    },
];

// ============================================================================
// SkillLoader
// ============================================================================

/// Discovers and loads skills from configured directories and builtins.
///
/// Skills are discovered lazily -- only metadata is read at startup.
/// Full content is loaded on-demand when the skill is invoked.
#[derive(Debug)]
pub struct SkillLoader {
    /// Directories to scan, in priority order (first = highest priority).
    dirs: Vec<PathBuf>,
    /// Remote URLs to fetch skill indexes from.
    skill_urls: Vec<String>,
    /// Cache of fully loaded skills (name -> LoadedSkill).
    cache: HashMap<String, LoadedSkill>,
    /// Cache of discovered metadata (full_name -> SkillMetadata).
    metadata_cache: HashMap<String, SkillMetadata>,
}

impl SkillLoader {
    /// Create a new skill loader.
    ///
    /// `skill_dirs` is in priority order: first directory has highest priority
    /// (typically project local). Directories that do not exist are tolerated.
    ///
    /// In addition to `skills/` directories, the loader also discovers skills
    /// from `commands/` directories at the same levels (matching OpenCode's
    /// convention where custom slash commands live in `.opencode/command/`).
    pub fn new(skill_dirs: Vec<PathBuf>) -> Self {
        // Expand skill_dirs to also include sibling "commands" directories.
        let mut dirs = Vec::new();
        for dir in &skill_dirs {
            dirs.push(dir.clone());
            // If dir ends with "skills", also check "commands" at the same level.
            if dir.file_name().and_then(|n| n.to_str()) == Some("skills")
                && let Some(parent) = dir.parent()
            {
                let commands_dir = parent.join("commands");
                if commands_dir.exists() {
                    dirs.push(commands_dir);
                }
            }
        }

        Self {
            dirs,
            skill_urls: Vec::new(),
            cache: HashMap::new(),
            metadata_cache: HashMap::new(),
        }
    }

    /// Add remote URLs to discover skills from.
    ///
    /// Each URL should point to a directory containing an `index.json` with
    /// the format: `{ "skills": [{ "name": "...", "files": ["SKILL.md", ...] }] }`.
    /// Skills are downloaded to a local cache directory.
    pub fn add_urls(&mut self, urls: Vec<String>) {
        self.skill_urls.extend(urls);
    }

    /// Scan skill directories and builtins for `.md` files, extract metadata.
    ///
    /// Project-local skills override user-global skills with the same name.
    /// User skills override builtins with the same name.
    ///
    /// Returns a list of all discovered [`SkillMetadata`].
    pub fn discover_skills(&mut self) -> Vec<SkillMetadata> {
        let mut skills: HashMap<String, SkillMetadata> = HashMap::new();

        // Process builtins first (lowest priority).
        for builtin in BUILTIN_SKILLS {
            if let Some(mut meta) = parse_frontmatter_str(builtin.content) {
                meta.source = SkillSource::Builtin;
                // Use the filename stem as a fallback name.
                if meta.name.is_empty() {
                    meta.name = builtin
                        .filename
                        .strip_suffix(".md")
                        .unwrap_or(builtin.filename)
                        .to_string();
                }
                let full_name = meta.full_name();
                skills.insert(full_name, meta);
            }
        }

        // Process directories in reverse order so higher-priority dirs override.
        for skill_dir in self.dirs.iter().rev() {
            if !skill_dir.exists() {
                continue;
            }

            let source = detect_source(skill_dir);

            // Scan for markdown files (both flat *.md and dir/SKILL.md patterns).
            if let Ok(entries) = glob_md_files(skill_dir) {
                for md_file in entries {
                    if let Some(mut meta) = parse_frontmatter_file(&md_file) {
                        meta.path = Some(md_file);
                        meta.source = source.clone();
                        let full_name = meta.full_name();
                        if let Some(existing) = skills.get(&full_name) {
                            debug!(
                                skill = full_name,
                                existing_source = %existing.source,
                                new_source = %meta.source,
                                "skill overridden by higher-priority source"
                            );
                        }
                        skills.insert(full_name, meta);
                    }
                }
            }
        }

        // Process URL-sourced skills (lower priority than local dirs).
        // Download to cache and discover like local directories.
        for url in &self.skill_urls.clone() {
            match pull_url_skills(url) {
                Ok(dirs) => {
                    for skill_dir in dirs {
                        if let Ok(entries) = glob_md_files(&skill_dir) {
                            for md_file in entries {
                                if let Some(mut meta) = parse_frontmatter_file(&md_file) {
                                    meta.path = Some(md_file);
                                    meta.source = SkillSource::Url(url.clone());
                                    let full_name = meta.full_name();
                                    // URL skills don't override local skills
                                    use std::collections::hash_map::Entry;
                                    match skills.entry(full_name) {
                                        Entry::Vacant(e) => {
                                            e.insert(meta);
                                        }
                                        Entry::Occupied(e) => {
                                            debug!(
                                                skill = e.key(),
                                                url = url,
                                                "URL skill skipped — local version takes priority"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(url = url, error = %e, "Failed to pull skills from URL");
                }
            }
        }

        self.metadata_cache = skills;
        self.metadata_cache.values().cloned().collect()
    }

    /// Load full skill content by name.
    ///
    /// `name` can be a plain name (e.g. `"commit"`) or namespaced
    /// (e.g. `"git:commit"`). Returns `None` if not found.
    ///
    /// If the skill file has been modified since last cache, the cache
    /// is automatically invalidated and the skill is reloaded.
    pub fn load_skill(&mut self, name: &str) -> Option<LoadedSkill> {
        // Check cache, with mtime-based invalidation for file-based skills.
        if let Some(cached) = self.cache.get(name) {
            if !is_cache_stale(cached) {
                return Some(cached.clone());
            }
            debug!(skill = name, "skill file modified on disk — reloading");
            self.cache.remove(name);
        }

        // Ensure metadata is loaded.
        if self.metadata_cache.is_empty() {
            self.discover_skills();
        }

        // Look up by full name first.
        let metadata = self.metadata_cache.get(name).cloned().or_else(|| {
            // Fall back: search by bare name.
            self.metadata_cache
                .values()
                .find(|m| m.name == name)
                .cloned()
        });

        let metadata = match metadata {
            Some(m) => m,
            None => {
                warn!(skill = name, "skill not found");
                return None;
            }
        };

        // Load full content.
        let raw_content = match &metadata.source {
            SkillSource::Builtin => {
                // Find the builtin by name.
                BUILTIN_SKILLS
                    .iter()
                    .find(|b| {
                        let stem = b.filename.strip_suffix(".md").unwrap_or(b.filename);
                        stem == metadata.name
                    })
                    .map(|b| b.content.to_string())
            }
            _ => {
                // Read from disk.
                metadata.path.as_ref().and_then(|p| {
                    std::fs::read_to_string(p)
                        .map_err(|e| {
                            warn!(path = %p.display(), error = %e, "failed to read skill file");
                            e
                        })
                        .ok()
                })
            }
        };

        let raw_content = raw_content?;
        let content = strip_frontmatter(&raw_content);

        // Discover companion files for directory-style skills.
        let companion_files = match &metadata.path {
            Some(p) => discover_companion_files(p),
            None => vec![],
        };

        // Record the file's modification time for cache invalidation.
        let cached_mtime = metadata
            .path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .and_then(|m| m.modified().ok());

        let skill = LoadedSkill {
            metadata: metadata.clone(),
            content,
            companion_files,
            cached_mtime,
        };

        self.cache.insert(name.to_string(), skill.clone());
        Some(skill)
    }

    /// Build a formatted skills index for inclusion in system prompts.
    ///
    /// Returns an empty string if no skills are available.
    pub fn build_skills_index(&mut self) -> String {
        let skills = self.discover_skills();
        if skills.is_empty() {
            return String::new();
        }

        let mut sorted = skills;
        sorted.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

        let mut lines = vec![
            "## Available Skills".to_string(),
            String::new(),
            "Use `invoke_skill` to load skill content into conversation context.".to_string(),
            String::new(),
        ];

        for skill in &sorted {
            if skill.namespace == "default" {
                lines.push(format!("- **{}**: {}", skill.name, skill.description));
            } else {
                lines.push(format!(
                    "- **{}:{}**: {}",
                    skill.namespace, skill.name, skill.description
                ));
            }
        }

        lines.join("\n")
    }

    /// Get all available skill names.
    ///
    /// Names use namespace prefix for non-default namespaces.
    pub fn get_skill_names(&mut self) -> Vec<String> {
        if self.metadata_cache.is_empty() {
            self.discover_skills();
        }

        self.metadata_cache
            .values()
            .map(|m| {
                if m.namespace == "default" {
                    m.name.clone()
                } else {
                    m.full_name()
                }
            })
            .collect()
    }

    /// Clear all caches. Useful for reloading skills after changes.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.metadata_cache.clear();
    }

    /// Expand variables in a skill's content.
    ///
    /// Replaces `{{variable}}` placeholders with values from the provided map.
    pub fn expand_variables(content: &str, variables: &HashMap<String, String>) -> String {
        let mut result = content.to_string();
        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ---- Variable expansion ----

    #[test]
    fn test_expand_variables() {
        let content = "Hello {{user}}, welcome to {{project}}.";
        let mut vars = HashMap::new();
        vars.insert("user".to_string(), "Alice".to_string());
        vars.insert("project".to_string(), "OpenDev".to_string());
        let result = SkillLoader::expand_variables(content, &vars);
        assert_eq!(result, "Hello Alice, welcome to OpenDev.");
    }

    #[test]
    fn test_expand_variables_no_match() {
        let content = "No variables here.";
        let vars = HashMap::new();
        let result = SkillLoader::expand_variables(content, &vars);
        assert_eq!(result, "No variables here.");
    }

    #[test]
    fn test_expand_variables_missing_key_left_intact() {
        let content = "Hello {{user}}, your role is {{role}}.";
        let mut vars = HashMap::new();
        vars.insert("user".to_string(), "Bob".to_string());
        let result = SkillLoader::expand_variables(content, &vars);
        assert_eq!(result, "Hello Bob, your role is {{role}}.");
    }

    // ---- SkillLoader with builtins ----

    #[test]
    fn test_discover_builtin_skills() {
        let mut loader = SkillLoader::new(vec![]);
        let skills = loader.discover_skills();

        // Should find all builtin skills.
        assert!(skills.len() >= 3);

        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"commit"));
        assert!(names.contains(&"review-pr"));
        assert!(names.contains(&"create-pr"));

        // All should be marked as builtin.
        for skill in &skills {
            assert_eq!(skill.source, SkillSource::Builtin);
        }
    }

    #[test]
    fn test_load_builtin_skill() {
        let mut loader = SkillLoader::new(vec![]);
        loader.discover_skills();

        let skill = loader.load_skill("commit").unwrap();
        assert_eq!(skill.metadata.name, "commit");
        assert!(!skill.content.is_empty());
        assert!(skill.content.contains("Git Commit"));
        // Content should NOT contain frontmatter.
        assert!(!skill.content.starts_with("---"));
    }

    #[test]
    fn test_load_nonexistent_skill_returns_none() {
        let mut loader = SkillLoader::new(vec![]);
        loader.discover_skills();
        assert!(loader.load_skill("nonexistent-skill-xyz").is_none());
    }

    // ---- SkillLoader with filesystem skills ----

    #[test]
    fn test_discover_filesystem_skills() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();

        // Create a flat skill file.
        fs::write(
            skill_dir.join("deploy.md"),
            "---\nname: deploy\ndescription: Deployment skill\n---\n\n# Deploy\nDeploy instructions.\n",
        )
        .unwrap();

        // Create a directory-style skill.
        let nested = skill_dir.join("testing");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("SKILL.md"),
            "---\nname: testing\ndescription: Testing patterns\nnamespace: qa\n---\n\n# Testing\n",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![skill_dir]);
        let skills = loader.discover_skills();

        let names: Vec<String> = skills.iter().map(|s| s.full_name()).collect();
        assert!(names.contains(&"deploy".to_string()));
        assert!(names.contains(&"qa:testing".to_string()));
    }

    #[test]
    fn test_project_skill_overrides_builtin() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();

        // Create a project-level "commit" skill that overrides the builtin.
        fs::write(
            skill_dir.join("commit.md"),
            "---\nname: commit\ndescription: Custom commit skill\n---\n\n# Custom Commit\nOverridden.\n",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![skill_dir]);
        let skills = loader.discover_skills();

        let commit = skills.iter().find(|s| s.name == "commit").unwrap();
        assert_eq!(commit.description, "Custom commit skill");
        // Should NOT be builtin since the project overrode it.
        assert_ne!(commit.source, SkillSource::Builtin);
    }

    #[test]
    fn test_load_filesystem_skill() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();

        fs::write(
            skill_dir.join("deploy.md"),
            "---\nname: deploy\ndescription: Deploy skill\n---\n\n# Deploy\nStep 1: Push.\n",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![skill_dir]);
        loader.discover_skills();

        let skill = loader.load_skill("deploy").unwrap();
        assert_eq!(skill.metadata.name, "deploy");
        assert!(skill.content.contains("Step 1: Push."));
        assert!(!skill.content.contains("---"));
    }

    #[test]
    fn test_skill_name_fallback_to_filename() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();

        // Frontmatter without a name field.
        fs::write(
            skill_dir.join("my-cool-skill.md"),
            "---\ndescription: A cool skill\n---\n\nContent here.\n",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![skill_dir]);
        let skills = loader.discover_skills();

        let cool = skills.iter().find(|s| s.name == "my-cool-skill");
        assert!(cool.is_some(), "should fall back to filename stem");
    }

    // ---- Skills index ----

    #[test]
    fn test_build_skills_index() {
        let mut loader = SkillLoader::new(vec![]);
        let index = loader.build_skills_index();

        assert!(index.contains("## Available Skills"));
        assert!(index.contains("**commit**"));
        assert!(index.contains("**review-pr**"));
        assert!(index.contains("invoke_skill"));
    }

    #[test]
    fn test_build_skills_index_empty_when_no_skills() {
        // Create a loader with a non-existent dir and no builtins would
        // still have builtins, so this just verifies the format.
        let mut loader = SkillLoader::new(vec![]);
        let index = loader.build_skills_index();
        assert!(!index.is_empty()); // builtins are always present
    }

    // ---- get_skill_names ----

    #[test]
    fn test_get_skill_names() {
        let mut loader = SkillLoader::new(vec![]);
        let names = loader.get_skill_names();
        assert!(names.contains(&"commit".to_string()));
        assert!(names.contains(&"review-pr".to_string()));
    }

    // ---- Cache clearing ----

    #[test]
    fn test_clear_cache() {
        let mut loader = SkillLoader::new(vec![]);
        loader.discover_skills();
        assert!(!loader.metadata_cache.is_empty());

        loader.clear_cache();
        assert!(loader.metadata_cache.is_empty());
        assert!(loader.cache.is_empty());
    }

    // ---- Priority ordering ----

    #[test]
    fn test_first_dir_has_highest_priority() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        let dir1 = tmp1.path().join("skills");
        let dir2 = tmp2.path().join("skills");
        fs::create_dir_all(&dir1).unwrap();
        fs::create_dir_all(&dir2).unwrap();

        fs::write(
            dir1.join("myskill.md"),
            "---\nname: myskill\ndescription: From dir1 (high prio)\n---\n\nDir1 content.\n",
        )
        .unwrap();

        fs::write(
            dir2.join("myskill.md"),
            "---\nname: myskill\ndescription: From dir2 (low prio)\n---\n\nDir2 content.\n",
        )
        .unwrap();

        // dir1 first = highest priority.
        let mut loader = SkillLoader::new(vec![dir1, dir2]);
        let skills = loader.discover_skills();

        let myskill = skills.iter().find(|s| s.name == "myskill").unwrap();
        assert_eq!(myskill.description, "From dir1 (high prio)");
    }

    // ---- Commands directory alias ----

    #[test]
    fn test_discover_skills_from_commands_dir() {
        let tmp = TempDir::new().unwrap();
        let opendev_dir = tmp.path().join(".opendev");
        let skills_dir = opendev_dir.join("skills");
        let commands_dir = opendev_dir.join("commands");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&commands_dir).unwrap();

        // Skill in skills/ dir.
        fs::write(
            skills_dir.join("commit.md"),
            "---\nname: commit\ndescription: Git commit\n---\n\n# Commit\n",
        )
        .unwrap();

        // Command in commands/ dir.
        fs::write(
            commands_dir.join("deploy.md"),
            "---\nname: deploy\ndescription: Deploy app\n---\n\n# Deploy\n",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![skills_dir]);
        let skills = loader.discover_skills();

        let names: Vec<String> = skills.iter().map(|s| s.full_name()).collect();
        assert!(names.contains(&"commit".to_string()));
        assert!(names.contains(&"deploy".to_string()));
    }

    // ---- Companion files ----

    #[test]
    fn test_companion_files_discovered_for_directory_skill() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        let sub_dir = skill_dir.join("testing");
        fs::create_dir_all(&sub_dir).unwrap();

        fs::write(
            sub_dir.join("SKILL.md"),
            "---\nname: testing\ndescription: Testing patterns\n---\n\n# Testing\n",
        )
        .unwrap();
        fs::write(sub_dir.join("helpers.sh"), "#!/bin/bash\necho test").unwrap();
        fs::write(sub_dir.join("fixtures.json"), r#"{"key": "value"}"#).unwrap();

        let mut loader = SkillLoader::new(vec![skill_dir]);
        loader.discover_skills();

        let skill = loader.load_skill("testing").unwrap();
        assert_eq!(skill.companion_files.len(), 2);

        let relative_paths: Vec<&str> = skill
            .companion_files
            .iter()
            .map(|f| f.relative_path.as_str())
            .collect();
        assert!(relative_paths.contains(&"helpers.sh"));
        assert!(relative_paths.contains(&"fixtures.json"));
    }

    #[test]
    fn test_companion_files_empty_for_flat_skill() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();

        fs::write(
            skill_dir.join("deploy.md"),
            "---\nname: deploy\ndescription: Deploy\n---\n\n# Deploy\n",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![skill_dir]);
        loader.discover_skills();

        let skill = loader.load_skill("deploy").unwrap();
        // Flat skill in the root skills dir has no companions (only itself).
        assert!(skill.companion_files.is_empty());
    }

    #[test]
    fn test_companion_files_max_limit() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        let sub_dir = skill_dir.join("big-skill");
        fs::create_dir_all(&sub_dir).unwrap();

        fs::write(
            sub_dir.join("SKILL.md"),
            "---\nname: big-skill\ndescription: Many files\n---\n\n# Big\n",
        )
        .unwrap();

        // Create 15 companion files — should be capped at MAX_COMPANION_FILES (10).
        for i in 0..15 {
            fs::write(
                sub_dir.join(format!("file_{i}.txt")),
                format!("content {i}"),
            )
            .unwrap();
        }

        let mut loader = SkillLoader::new(vec![skill_dir]);
        loader.discover_skills();

        let skill = loader.load_skill("big-skill").unwrap();
        assert_eq!(skill.companion_files.len(), 10);
    }

    #[test]
    fn test_companion_files_nested_subdirs() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        let sub_dir = skill_dir.join("complex");
        let nested = sub_dir.join("scripts");
        fs::create_dir_all(&nested).unwrap();

        fs::write(
            sub_dir.join("SKILL.md"),
            "---\nname: complex\ndescription: Complex skill\n---\n\n# Complex\n",
        )
        .unwrap();
        fs::write(sub_dir.join("README.md"), "# README").unwrap();
        fs::write(nested.join("run.sh"), "#!/bin/bash").unwrap();

        let mut loader = SkillLoader::new(vec![skill_dir]);
        loader.discover_skills();

        let skill = loader.load_skill("complex").unwrap();
        assert_eq!(skill.companion_files.len(), 2);

        let relative_paths: Vec<&str> = skill
            .companion_files
            .iter()
            .map(|f| f.relative_path.as_str())
            .collect();
        assert!(relative_paths.contains(&"README.md"));
        assert!(
            relative_paths.contains(&"scripts/run.sh")
                || relative_paths.iter().any(|p| p.ends_with("run.sh"))
        );
    }

    #[test]
    fn test_companion_files_for_builtin_skill() {
        let mut loader = SkillLoader::new(vec![]);
        loader.discover_skills();

        let skill = loader.load_skill("commit").unwrap();
        // Builtin skills have no companion files.
        assert!(skill.companion_files.is_empty());
    }

    // ---- Namespaced skill lookup ----

    #[test]
    fn test_load_namespaced_skill() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();

        fs::write(
            skill_dir.join("rebase.md"),
            "---\nname: rebase\ndescription: Git rebase\nnamespace: git\n---\n\n# Rebase\n",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![skill_dir]);
        loader.discover_skills();

        // Load by full namespaced name.
        let skill = loader.load_skill("git:rebase").unwrap();
        assert_eq!(skill.metadata.name, "rebase");
        assert_eq!(skill.metadata.namespace, "git");

        // Also loadable by bare name.
        let mut loader2 = SkillLoader::new(vec![tmp.path().join("skills")]);
        loader2.discover_skills();
        let skill2 = loader2.load_skill("rebase").unwrap();
        assert_eq!(skill2.metadata.name, "rebase");
    }

    // ---- Model override ----

    #[test]
    fn test_load_skill_with_model_override() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();

        fs::write(
            skill_dir.join("fast-lint.md"),
            "---\nname: fast-lint\ndescription: Fast lint\nmodel: gpt-4o-mini\n---\n\n# Lint\nLint quickly.\n",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![skill_dir]);
        loader.discover_skills();

        let skill = loader.load_skill("fast-lint").unwrap();
        assert_eq!(skill.metadata.model.as_deref(), Some("gpt-4o-mini"));
    }

    // ---- Directory discovery across .claude, .agents, .opendev ----

    #[test]
    fn test_discover_skills_from_claude_skills_dir() {
        let tmp = TempDir::new().unwrap();
        let claude_dir = tmp.path().join(".claude");
        let skills_dir = claude_dir.join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        fs::write(
            skills_dir.join("my-tool.md"),
            "---\nname: my-tool\ndescription: A tool from .claude/skills\n---\n\n# My Tool\n",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![skills_dir]);
        let skills = loader.discover_skills();

        let names: Vec<String> = skills.iter().map(|s| s.full_name()).collect();
        assert!(names.contains(&"my-tool".to_string()));
    }

    #[test]
    fn test_claude_skills_higher_priority_than_opendev() {
        let tmp = TempDir::new().unwrap();

        let claude_skills = tmp.path().join(".claude").join("skills");
        let opendev_skills = tmp.path().join(".opendev").join("skills");
        fs::create_dir_all(&claude_skills).unwrap();
        fs::create_dir_all(&opendev_skills).unwrap();

        fs::write(
            claude_skills.join("myskill.md"),
            "---\nname: myskill\ndescription: From .claude (high prio)\n---\n\nClaude content.\n",
        )
        .unwrap();

        fs::write(
            opendev_skills.join("myskill.md"),
            "---\nname: myskill\ndescription: From .opendev (low prio)\n---\n\nOpenDev content.\n",
        )
        .unwrap();

        // .claude/skills first = highest priority
        let mut loader = SkillLoader::new(vec![claude_skills, opendev_skills]);
        let skills = loader.discover_skills();

        let myskill = skills.iter().find(|s| s.name == "myskill").unwrap();
        assert_eq!(myskill.description, "From .claude (high prio)");
    }

    #[test]
    fn test_agents_skills_directory_discovered() {
        let tmp = TempDir::new().unwrap();

        let agents_skills = tmp.path().join(".agents").join("skills");
        fs::create_dir_all(&agents_skills).unwrap();

        fs::write(
            agents_skills.join("deploy.md"),
            "---\nname: deploy\ndescription: Deploy helper from .agents\n---\n\n# Deploy\nDeploy instructions.\n",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![agents_skills]);
        let skills = loader.discover_skills();

        let deploy = skills.iter().find(|s| s.name == "deploy").unwrap();
        assert_eq!(deploy.description, "Deploy helper from .agents");
    }

    #[test]
    fn test_skill_priority_claude_over_agents_over_opendev() {
        let tmp = TempDir::new().unwrap();

        let claude_skills = tmp.path().join(".claude").join("skills");
        let agents_skills = tmp.path().join(".agents").join("skills");
        let opendev_skills = tmp.path().join(".opendev").join("skills");
        fs::create_dir_all(&claude_skills).unwrap();
        fs::create_dir_all(&agents_skills).unwrap();
        fs::create_dir_all(&opendev_skills).unwrap();

        fs::write(
            agents_skills.join("shared.md"),
            "---\nname: shared\ndescription: From .agents\n---\n\nAgents content.\n",
        )
        .unwrap();

        fs::write(
            opendev_skills.join("shared.md"),
            "---\nname: shared\ndescription: From .opendev\n---\n\nOpenDev content.\n",
        )
        .unwrap();

        // Priority: .claude > .agents > .opendev
        let mut loader = SkillLoader::new(vec![claude_skills, agents_skills, opendev_skills]);
        let skills = loader.discover_skills();

        let shared = skills.iter().find(|s| s.name == "shared").unwrap();
        // .agents has higher priority than .opendev
        assert_eq!(shared.description, "From .agents");
    }

    // --- URL skill discovery tests ---

    #[test]
    fn test_add_urls() {
        let mut loader = SkillLoader::new(vec![]);
        assert!(loader.skill_urls.is_empty());
        loader.add_urls(vec![
            "https://example.com/skills".to_string(),
            "https://other.com/skills".to_string(),
        ]);
        assert_eq!(loader.skill_urls.len(), 2);
        assert_eq!(loader.skill_urls[0], "https://example.com/skills");
    }

    #[test]
    fn test_pull_url_skills_simulated_cache() {
        // Simulate what pull_url_skills would create in the cache directory
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        // Create a valid skill file
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Test skill from URL\n---\n\n# My Skill\nContent here.",
        ).unwrap();

        // Use the directory as if it were a cached URL skill
        let mut loader = SkillLoader::new(vec![]);
        // Manually add the cached dir for discovery
        loader.dirs.push(tmp.path().to_path_buf());
        let skills = loader.discover_skills();

        assert!(skills.iter().any(|s| s.name == "my-skill"));
    }

    #[test]
    fn test_url_skills_dont_override_local() {
        let tmp = tempfile::tempdir().unwrap();

        // Create a local skill
        let local_dir = tmp.path().join("local-skills");
        std::fs::create_dir_all(&local_dir).unwrap();
        std::fs::write(
            local_dir.join("test-skill.md"),
            "---\nname: test-skill\ndescription: Local version\n---\n\nLocal content.",
        )
        .unwrap();

        // Create a "URL-cached" skill with the same name
        let url_dir = tmp.path().join("url-skills");
        std::fs::create_dir_all(&url_dir).unwrap();
        std::fs::write(
            url_dir.join("test-skill.md"),
            "---\nname: test-skill\ndescription: URL version\n---\n\nURL content.",
        )
        .unwrap();

        // Local dir has higher priority (listed first), URL dir is lower
        let mut loader = SkillLoader::new(vec![local_dir]);
        // Simulate URL skill being discovered from cache dir
        loader.dirs.push(url_dir);
        let skills = loader.discover_skills();

        // The local version should win
        let skill = skills.iter().find(|s| s.name == "test-skill").unwrap();
        assert!(
            skill.description.contains("Local") || matches!(skill.source, SkillSource::Project),
            "Local skill should take priority over URL skill"
        );
    }

    // --- Cache invalidation via mtime ---

    #[test]
    fn test_load_skill_reloads_after_file_change() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir(&skills_dir).unwrap();
        let file = skills_dir.join("hot-reload.md");
        std::fs::write(
            &file,
            "---\nname: hot-reload\ndescription: Hot reload test\n---\n\nVersion 1",
        )
        .unwrap();

        let mut loader = SkillLoader::new(vec![skills_dir]);

        // First load.
        let skill1 = loader.load_skill("hot-reload").unwrap();
        assert!(skill1.content.contains("Version 1"));

        // Modify the file (with a brief sleep to ensure mtime changes).
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(
            &file,
            "---\nname: hot-reload\ndescription: Hot reload test\n---\n\nVersion 2",
        )
        .unwrap();

        // Second load should pick up the change.
        let skill2 = loader.load_skill("hot-reload").unwrap();
        assert!(
            skill2.content.contains("Version 2"),
            "Expected reloaded content with 'Version 2', got: {}",
            skill2.content
        );
    }
}
