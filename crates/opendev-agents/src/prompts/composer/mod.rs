//! Prompt composition engine with conditional loading.
//!
//! Mirrors `opendev/core/agents/prompts/composition.py`.
//!
//! Composes system prompts from modular sections based on runtime context
//! and conversation lifecycle. Supports priority ordering, conditional
//! inclusion, cache-aware two-part splitting, and variable substitution.
//!
//! Templates are resolved in order:
//! 1. Embedded (compile-time `include_str!`) — zero filesystem dependency
//! 2. Filesystem fallback (`templates_dir`) — for user customisation

mod conditions;
mod factories;

use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;

use super::embedded;

// Re-export public items from submodules
pub use conditions::{ctx_bool, ctx_eq, ctx_in, ctx_present};
pub use factories::{create_composer, create_default_composer};

/// Regex to strip HTML comment frontmatter from markdown files.
static FRONTMATTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)^\s*<!--.*?-->\s*").expect("valid regex: frontmatter pattern")
});

/// Regex for `{{variable_name}}` placeholders in templates.
static VARIABLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{(\w+)\}\}").expect("valid regex: variable placeholder pattern")
});

/// Runtime context passed to condition functions for section filtering.
pub type PromptContext = HashMap<String, serde_json::Value>;

/// A condition function that determines if a section should be included.
pub type ConditionFn = Box<dyn Fn(&PromptContext) -> bool + Send + Sync>;

/// A section to conditionally include in the system prompt.
pub struct PromptSection {
    /// Section identifier.
    pub name: String,
    /// Path to template file (relative to templates_dir).
    pub file_path: String,
    /// Optional predicate to determine if section should be included.
    pub condition: Option<ConditionFn>,
    /// Loading priority (lower = earlier in prompt).
    pub priority: i32,
    /// Whether this section is stable across turns (true = cacheable).
    pub cacheable: bool,
}

impl std::fmt::Debug for PromptSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptSection")
            .field("name", &self.name)
            .field("file_path", &self.file_path)
            .field("priority", &self.priority)
            .field("cacheable", &self.cacheable)
            .field("has_condition", &self.condition.is_some())
            .finish()
    }
}

/// Composes system prompts from modular sections.
///
/// Follows Claude Code's approach of building prompts from many small
/// markdown files with conditional loading based on runtime context.
///
/// Templates are resolved first from the embedded store (compile-time),
/// falling back to the filesystem `templates_dir` for user overrides.
#[derive(Debug)]
pub struct PromptComposer {
    templates_dir: PathBuf,
    sections: Vec<PromptSection>,
}

impl PromptComposer {
    /// Create a new composer.
    pub fn new(templates_dir: impl Into<PathBuf>) -> Self {
        Self {
            templates_dir: templates_dir.into(),
            sections: Vec::new(),
        }
    }

    /// Register a prompt section for conditional inclusion.
    pub fn register_section(
        &mut self,
        name: impl Into<String>,
        file_path: impl Into<String>,
        condition: Option<ConditionFn>,
        priority: i32,
        cacheable: bool,
    ) {
        self.sections.push(PromptSection {
            name: name.into(),
            file_path: file_path.into(),
            condition,
            priority,
            cacheable,
        });
    }

    /// Register a section with defaults (priority=50, cacheable=true, no condition).
    pub fn register_simple(&mut self, name: impl Into<String>, file_path: impl Into<String>) {
        self.register_section(name, file_path, None, 50, true);
    }

    /// Compose final prompt from registered sections.
    ///
    /// Sections are filtered by their condition, sorted by priority, loaded
    /// (embedded first, then filesystem), and joined with double newlines.
    pub fn compose(&self, context: &PromptContext) -> String {
        let included = self.filter_and_sort(context);
        let parts: Vec<String> = included
            .iter()
            .filter_map(|s| self.load_section_content(s))
            .collect();
        parts.join("\n\n")
    }

    /// Compose final prompt with variable substitution.
    ///
    /// After composing, replaces all `{{variable_name}}` placeholders with
    /// values from the provided variables map.
    pub fn compose_with_vars(
        &self,
        context: &PromptContext,
        variables: &HashMap<String, String>,
    ) -> String {
        let raw = self.compose(context);
        substitute_variables(&raw, variables)
    }

    /// Compose prompt split into stable (cacheable) and dynamic parts.
    ///
    /// For Anthropic prompt caching: the stable part gets cache_control,
    /// the dynamic part changes per session/turn.
    pub fn compose_two_part(&self, context: &PromptContext) -> (String, String) {
        let included = self.filter_and_sort(context);
        let mut stable_parts = Vec::new();
        let mut dynamic_parts = Vec::new();

        for section in &included {
            if let Some(content) = self.load_section_content(section) {
                if section.cacheable {
                    stable_parts.push(content);
                } else {
                    dynamic_parts.push(content);
                }
            }
        }

        (stable_parts.join("\n\n"), dynamic_parts.join("\n\n"))
    }

    /// Compose two-part prompt with variable substitution on both halves.
    pub fn compose_two_part_with_vars(
        &self,
        context: &PromptContext,
        variables: &HashMap<String, String>,
    ) -> (String, String) {
        let (stable, dynamic) = self.compose_two_part(context);
        (
            substitute_variables(&stable, variables),
            substitute_variables(&dynamic, variables),
        )
    }

    /// Get the number of registered sections.
    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    /// Get names of all registered sections.
    pub fn section_names(&self) -> Vec<&str> {
        self.sections.iter().map(|s| s.name.as_str()).collect()
    }

    fn filter_and_sort(&self, context: &PromptContext) -> Vec<&PromptSection> {
        let mut included: Vec<&PromptSection> = self
            .sections
            .iter()
            .filter(|s| s.condition.as_ref().is_none_or(|f| f(context)))
            .collect();
        included.sort_by_key(|s| s.priority);
        included
    }

    /// Load a section's content: try embedded first, then filesystem.
    fn load_section_content(&self, section: &PromptSection) -> Option<String> {
        // 1. Try embedded templates
        if let Some(raw) = embedded::get_embedded(&section.file_path) {
            let stripped = strip_frontmatter(raw);
            if !stripped.is_empty() {
                return Some(stripped);
            }
        }

        // 2. Fallback to filesystem
        let file_path = self.templates_dir.join(&section.file_path);
        if !file_path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(&file_path).ok()?;
        let stripped = strip_frontmatter(&content);
        if stripped.is_empty() {
            None
        } else {
            Some(stripped)
        }
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Strip HTML comment frontmatter from markdown content.
pub fn strip_frontmatter(content: &str) -> String {
    FRONTMATTER_RE.replace(content, "").trim().to_string()
}

/// Substitute `{{variable_name}}` placeholders in a template string.
///
/// Variables not present in the map are left as-is.
///
/// ```
/// use std::collections::HashMap;
/// use opendev_agents::prompts::substitute_variables;
///
/// let mut vars = HashMap::new();
/// vars.insert("session_id".into(), "abc-123".into());
/// let result = substitute_variables("path: ~/.opendev/sessions/{{session_id}}/", &vars);
/// assert_eq!(result, "path: ~/.opendev/sessions/abc-123/");
/// ```
pub fn substitute_variables(template: &str, variables: &HashMap<String, String>) -> String {
    VARIABLE_RE
        .replace_all(template, |caps: &regex::Captures| {
            let key = &caps[1];
            variables
                .get(key)
                .cloned()
                .unwrap_or_else(|| caps[0].to_string())
        })
        .into_owned()
}

#[cfg(test)]
mod tests;
