//! SkillLoader: discovers and loads skills from directories, URLs, and builtins.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use globset::{Glob, GlobSet, GlobSetBuilder};
use tracing::{debug, warn};

use super::builtins::BUILTIN_SKILLS;
use super::discovery::{
    detect_source, discover_companion_files, glob_md_files, is_cache_stale, pull_url_skills,
};
use super::metadata::{LoadedSkill, SkillMetadata, SkillSource};
use super::parsing::{parse_frontmatter_file, parse_frontmatter_str, strip_frontmatter};

/// Maximum characters per skill description in the listing.
const MAX_DESC_CHARS: usize = 200;

/// Discovers and loads skills from configured directories and builtins.
///
/// Skills are discovered lazily -- only metadata is read at startup.
/// Full content is loaded on-demand when the skill is invoked.
///
/// Supports conditional activation via `paths` globs: skills with path
/// patterns are hidden until matching files are touched in the session.
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
    /// Files touched in the current session — used for conditional skill activation.
    touched_files: HashSet<String>,
    /// Compiled glob matchers keyed by skill full_name.
    path_matchers: HashMap<String, GlobSet>,
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
            touched_files: HashSet::new(),
            path_matchers: HashMap::new(),
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
        self.compile_path_matchers();
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

    // ============================================================================
    // Conditional activation via paths
    // ============================================================================

    /// Compile glob matchers for all skills with `paths` patterns.
    fn compile_path_matchers(&mut self) {
        self.path_matchers.clear();
        for (full_name, meta) in &self.metadata_cache {
            if meta.paths.is_empty() {
                continue;
            }
            let mut builder = GlobSetBuilder::new();
            for pattern in &meta.paths {
                match Glob::new(pattern) {
                    Ok(g) => {
                        builder.add(g);
                    }
                    Err(e) => {
                        warn!(
                            skill = full_name,
                            pattern = pattern,
                            error = %e,
                            "invalid glob pattern in skill paths"
                        );
                    }
                }
            }
            if let Ok(globset) = builder.build() {
                self.path_matchers.insert(full_name.clone(), globset);
            }
        }
    }

    /// Notify the loader that a file was touched in the session.
    ///
    /// This enables conditional activation: skills with `paths` patterns
    /// become visible once a matching file is touched.
    ///
    /// Returns the names of skills that were newly activated.
    pub fn notify_file_touched(&mut self, path: &str) -> Vec<String> {
        self.touched_files.insert(path.to_string());
        // Check which skills are now activated by this file.
        let mut newly_activated = Vec::new();
        for (full_name, globset) in &self.path_matchers {
            if globset.is_match(path) {
                newly_activated.push(full_name.clone());
            }
        }
        newly_activated
    }

    /// Notify the loader that multiple files were touched.
    pub fn notify_files_touched(&mut self, paths: &[String]) -> Vec<String> {
        let mut activated = Vec::new();
        for path in paths {
            activated.extend(self.notify_file_touched(path));
        }
        activated.sort();
        activated.dedup();
        activated
    }

    /// Check if a skill is currently active (visible to the model).
    ///
    /// A skill is active if:
    /// - It has no `paths` patterns (always active), OR
    /// - At least one touched file matches its `paths` patterns.
    pub fn is_skill_active(&self, meta: &SkillMetadata) -> bool {
        if meta.paths.is_empty() {
            return true;
        }
        let full_name = meta.full_name();
        let globset = match self.path_matchers.get(&full_name) {
            Some(gs) => gs,
            None => return true, // No compiled matcher = always active
        };
        self.touched_files.iter().any(|f| globset.is_match(f))
    }

    /// Reset session state (touched files). Call on new session.
    pub fn clear_session_state(&mut self) {
        self.touched_files.clear();
    }

    // ============================================================================
    // Skill loading
    // ============================================================================

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

    // ============================================================================
    // Skill index & listing
    // ============================================================================

    /// Build a formatted skills index for inclusion in system prompts.
    ///
    /// Only includes active skills (conditional skills must have matching
    /// touched files). Skills with `disable_model_invocation` are excluded.
    ///
    /// Returns an empty string if no skills are available.
    pub fn build_skills_index(&mut self) -> String {
        let skills = self.discover_skills();
        if skills.is_empty() {
            return String::new();
        }

        let active: Vec<&SkillMetadata> = skills
            .iter()
            .filter(|s| self.is_skill_active(s))
            .filter(|s| !s.disable_model_invocation)
            .collect();

        if active.is_empty() {
            return String::new();
        }

        let mut sorted = active;
        sorted.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

        let mut lines = vec![
            "## Available Skills".to_string(),
            String::new(),
            "Use `Skill` to load skill content into conversation context.".to_string(),
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

    /// Build a token-budgeted skills index.
    ///
    /// Allocates 1% of `context_tokens` for the skill listing. Builtins get
    /// full descriptions (tier 1), non-builtins get truncated (tier 2), and
    /// overflow skills are listed name-only (tier 3).
    pub fn build_skills_index_budgeted(&mut self, context_tokens: usize) -> String {
        let skills = self.discover_skills();
        if skills.is_empty() {
            return String::new();
        }

        let active: Vec<&SkillMetadata> = skills
            .iter()
            .filter(|s| self.is_skill_active(s))
            .filter(|s| !s.disable_model_invocation)
            .collect();

        if active.is_empty() {
            return String::new();
        }

        let budget_chars = (context_tokens / 100) * 4; // 1% of context, ~4 chars/token
        let budget_chars = budget_chars.max(500); // Minimum 500 chars

        let mut sorted = active;
        sorted.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

        let header = "## Available Skills\n\nUse `Skill` to load skill content into conversation context.\n\n";
        let mut output = header.to_string();
        let mut used_chars = output.len();

        for skill in &sorted {
            let name_part = if skill.namespace == "default" {
                format!("**{}**", skill.name)
            } else {
                format!("**{}:{}**", skill.namespace, skill.name)
            };

            // Tier 1: builtin = full description
            // Tier 2: non-builtin = truncated description
            let desc = if skill.source == SkillSource::Builtin {
                truncate_desc(&skill.description, MAX_DESC_CHARS)
            } else {
                truncate_desc(&skill.description, MAX_DESC_CHARS / 2)
            };

            let full_line = format!("- {name_part}: {desc}\n");
            let name_only_line = format!("- {name_part}\n");

            if used_chars + full_line.len() <= budget_chars {
                output.push_str(&full_line);
                used_chars += full_line.len();
            } else if used_chars + name_only_line.len() <= budget_chars {
                // Tier 3: name only
                output.push_str(&name_only_line);
                used_chars += name_only_line.len();
            } else {
                // Over budget — stop
                break;
            }
        }

        output.trim_end().to_string()
    }

    /// Get all available skill names (active + model-visible only).
    ///
    /// Names use namespace prefix for non-default namespaces.
    pub fn get_skill_names(&mut self) -> Vec<String> {
        if self.metadata_cache.is_empty() {
            self.discover_skills();
        }

        self.metadata_cache
            .values()
            .filter(|m| self.is_skill_active(m))
            .filter(|m| !m.disable_model_invocation)
            .map(|m| {
                if m.namespace == "default" {
                    m.name.clone()
                } else {
                    m.full_name()
                }
            })
            .collect()
    }

    /// Get names of all user-invocable skills (for slash command autocomplete).
    pub fn get_user_invocable_skill_names(&mut self) -> Vec<String> {
        if self.metadata_cache.is_empty() {
            self.discover_skills();
        }

        self.metadata_cache
            .values()
            .filter(|m| m.user_invocable)
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
        self.path_matchers.clear();
    }

    // ============================================================================
    // Variable expansion
    // ============================================================================

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

    /// Expand `${VARIABLE}` and `$VARIABLE` patterns with runtime values.
    pub fn expand_dollar_variables(content: &str, variables: &HashMap<String, String>) -> String {
        let mut result = content.to_string();
        for (key, value) in variables {
            // ${VARIABLE} syntax
            let braced = format!("${{{key}}}");
            result = result.replace(&braced, value);
            // $VARIABLE syntax (word boundary: followed by non-alphanumeric or end)
            let dollar = format!("${key}");
            result = result.replace(&dollar, value);
        }
        result
    }

    /// Build runtime variables for a loaded skill.
    ///
    /// Provides `SKILL_DIR`, `SESSION_ID`, and `WORKING_DIR`.
    pub fn build_runtime_variables(
        skill: &LoadedSkill,
        session_id: &str,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        if let Some(ref path) = skill.metadata.path
            && let Some(dir) = path.parent()
        {
            vars.insert("SKILL_DIR".into(), dir.display().to_string());
        }
        vars.insert("SESSION_ID".into(), session_id.to_string());
        if let Ok(cwd) = std::env::current_dir() {
            vars.insert("WORKING_DIR".into(), cwd.display().to_string());
        }
        vars
    }
}

/// Truncate a description string to `max_chars`, ending at a word boundary.
fn truncate_desc(desc: &str, max_chars: usize) -> String {
    if desc.len() <= max_chars {
        return desc.to_string();
    }
    // Find the last space before max_chars.
    let truncated = &desc[..max_chars];
    match truncated.rfind(' ') {
        Some(pos) => format!("{}...", &desc[..pos]),
        None => format!("{truncated}..."),
    }
}

#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;
