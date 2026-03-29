//! SkillLoader: discovers and loads skills from directories, URLs, and builtins.

use std::collections::HashMap;
use std::path::PathBuf;

use tracing::{debug, warn};

use super::builtins::BUILTIN_SKILLS;
use super::discovery::{
    detect_source, discover_companion_files, glob_md_files, is_cache_stale, pull_url_skills,
};
use super::metadata::{LoadedSkill, SkillMetadata, SkillSource};
use super::parsing::{parse_frontmatter_file, parse_frontmatter_str, strip_frontmatter};

/// Discovers and loads skills from configured directories and builtins.
///
/// Skills are discovered lazily -- only metadata is read at startup.
/// Full content is loaded on-demand when the skill is invoked.
#[derive(Debug)]
pub struct SkillLoader {
    /// Directories to scan, in priority order (first = highest priority).
    pub(crate) dirs: Vec<PathBuf>,
    /// Remote URLs to fetch skill indexes from.
    pub(crate) skill_urls: Vec<String>,
    /// Cache of fully loaded skills (name -> LoadedSkill).
    cache: HashMap<String, LoadedSkill>,
    /// Cache of discovered metadata (full_name -> SkillMetadata).
    pub(crate) metadata_cache: HashMap<String, SkillMetadata>,
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
        // Builtins have lowest priority, then remote URLs, then local directories.
        let mut skills = self.discover_builtins();
        let remote = self.discover_remote();
        let local = self.discover_local();

        // Remote skills only fill in gaps (don't override builtins or local).
        for (full_name, meta) in remote {
            use std::collections::hash_map::Entry;
            match skills.entry(full_name) {
                Entry::Vacant(e) => {
                    e.insert(meta);
                }
                Entry::Occupied(e) => {
                    debug!(
                        skill = e.key(),
                        "remote skill skipped — existing version takes priority"
                    );
                }
            }
        }

        // Local directory skills override everything.
        skills.extend(local);

        self.metadata_cache = skills;
        self.metadata_cache.values().cloned().collect()
    }

    /// Discover embedded builtin skills.
    fn discover_builtins(&self) -> HashMap<String, SkillMetadata> {
        let mut skills = HashMap::new();
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
        skills
    }

    /// Discover skills from local filesystem directories.
    ///
    /// Directories are processed in reverse order so higher-priority dirs
    /// (listed first in `self.dirs`) override lower-priority ones.
    fn discover_local(&self) -> HashMap<String, SkillMetadata> {
        let mut skills: HashMap<String, SkillMetadata> = HashMap::new();
        // Process in reverse so higher-priority dirs override.
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
        skills
    }

    /// Discover skills from remote URLs.
    ///
    /// Each URL is fetched and its skills are downloaded to a local cache.
    /// Remote skills have lower priority than local directory skills.
    fn discover_remote(&self) -> HashMap<String, SkillMetadata> {
        let mut skills = HashMap::new();
        for url in &self.skill_urls {
            match pull_url_skills(url) {
                Ok(dirs) => {
                    for skill_dir in dirs {
                        if let Ok(entries) = glob_md_files(&skill_dir) {
                            for md_file in entries {
                                if let Some(mut meta) = parse_frontmatter_file(&md_file) {
                                    meta.path = Some(md_file);
                                    meta.source = SkillSource::Url(url.clone());
                                    let full_name = meta.full_name();
                                    // Don't override skills already found from other URLs.
                                    use std::collections::hash_map::Entry;
                                    match skills.entry(full_name) {
                                        Entry::Vacant(e) => {
                                            e.insert(meta);
                                        }
                                        Entry::Occupied(e) => {
                                            debug!(
                                                skill = e.key(),
                                                url = url,
                                                "URL skill skipped — earlier URL takes priority"
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
        skills
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
#[path = "loader_tests.rs"]
mod tests;
