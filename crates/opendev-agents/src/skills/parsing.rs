//! Frontmatter and YAML parsing for skill files.
//!
//! Extracts metadata from YAML frontmatter blocks and provides
//! simple key-value YAML parsing without a full YAML library.

use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use tracing::debug;

use super::metadata::{SkillMetadata, SkillSource};

/// Parse frontmatter from a file on disk.
pub(super) fn parse_frontmatter_file(path: &Path) -> Option<SkillMetadata> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            debug!(path = %path.display(), error = %e, "failed to read skill file");
            return None;
        }
    };
    let mut meta = parse_frontmatter_str(&content)?;
    if meta.name.is_empty() {
        // Fall back to filename stem.
        meta.name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
    }
    Some(meta)
}

/// Parse YAML frontmatter from a string.
///
/// Expects the format:
/// ```text
/// ---
/// name: foo
/// description: bar
/// namespace: baz
/// ---
/// ```
pub(super) fn parse_frontmatter_str(content: &str) -> Option<SkillMetadata> {
    let re = Regex::new(r"(?s)^---\n(.*?)\n---").ok()?;
    let caps = re.captures(content)?;
    let frontmatter = caps.get(1)?.as_str();

    // Simple key-value parsing (handles the common case without a full YAML parser).
    let data = parse_simple_yaml(frontmatter);

    let name = data.get("name").cloned().unwrap_or_default();
    let description = data
        .get("description")
        .cloned()
        .unwrap_or_else(|| format!("Skill: {}", if name.is_empty() { "unknown" } else { &name }));
    let namespace = data
        .get("namespace")
        .cloned()
        .unwrap_or_else(|| "default".to_string());

    let model = data.get("model").cloned().filter(|s| !s.is_empty());
    let agent = data.get("agent").cloned().filter(|s| !s.is_empty());

    Some(SkillMetadata {
        name,
        description,
        namespace,
        path: None,
        source: SkillSource::Builtin,
        model,
        agent,
    })
}

/// Simple YAML-like key:value parser for frontmatter.
///
/// Only handles flat `key: value` pairs. Strips surrounding quotes from values.
pub(super) fn parse_simple_yaml(text: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim().to_string();
            let mut value = value.trim().to_string();
            // Strip surrounding quotes.
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value = value[1..value.len() - 1].to_string();
            }
            result.insert(key, value);
        }
    }
    result
}

/// Strip YAML frontmatter from markdown content, returning the body.
pub(super) fn strip_frontmatter(content: &str) -> String {
    let re = match Regex::new(r"(?s)^---\n.*?\n---\n*") {
        Ok(r) => r,
        Err(_) => return content.to_string(),
    };
    re.replace(content, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Frontmatter parsing ----

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = "---\nname: commit\ndescription: Git commit skill\n---\n\n# Commit\n";
        let meta = parse_frontmatter_str(content).unwrap();
        assert_eq!(meta.name, "commit");
        assert_eq!(meta.description, "Git commit skill");
        assert_eq!(meta.namespace, "default");
    }

    #[test]
    fn test_parse_frontmatter_with_namespace() {
        let content = "---\nname: rebase\ndescription: Rebase skill\nnamespace: git\n---\n\nBody\n";
        let meta = parse_frontmatter_str(content).unwrap();
        assert_eq!(meta.name, "rebase");
        assert_eq!(meta.namespace, "git");
    }

    #[test]
    fn test_parse_frontmatter_quoted_values() {
        let content = "---\nname: \"my-skill\"\ndescription: 'Use when testing'\n---\n\nBody\n";
        let meta = parse_frontmatter_str(content).unwrap();
        assert_eq!(meta.name, "my-skill");
        assert_eq!(meta.description, "Use when testing");
    }

    #[test]
    fn test_parse_frontmatter_missing_returns_none() {
        let content = "# No frontmatter here\nJust a plain markdown file.\n";
        assert!(parse_frontmatter_str(content).is_none());
    }

    #[test]
    fn test_parse_frontmatter_empty_name_fallback() {
        let content = "---\ndescription: Some skill\n---\n\nBody\n";
        let meta = parse_frontmatter_str(content).unwrap();
        assert!(meta.name.is_empty()); // caller (parse_frontmatter_file) fills in
        assert_eq!(meta.description, "Some skill");
    }

    // ---- Strip frontmatter ----

    #[test]
    fn test_strip_frontmatter() {
        let content = "---\nname: foo\n---\n\n# Title\nBody text.";
        let body = strip_frontmatter(content);
        assert!(body.starts_with("# Title"));
        assert!(!body.contains("---"));
    }

    #[test]
    fn test_strip_frontmatter_no_frontmatter() {
        let content = "# Just markdown\nNo frontmatter.";
        let body = strip_frontmatter(content);
        assert_eq!(body, content);
    }

    // ---- Simple YAML parser ----

    #[test]
    fn test_parse_simple_yaml() {
        let text = "name: commit\ndescription: \"Git commit\"\n# comment\nnamespace: git";
        let data = parse_simple_yaml(text);
        assert_eq!(data.get("name").unwrap(), "commit");
        assert_eq!(data.get("description").unwrap(), "Git commit");
        assert_eq!(data.get("namespace").unwrap(), "git");
    }

    #[test]
    fn test_parse_simple_yaml_single_quotes() {
        let text = "name: 'my-skill'";
        let data = parse_simple_yaml(text);
        assert_eq!(data.get("name").unwrap(), "my-skill");
    }

    // ---- Model/agent in frontmatter ----

    #[test]
    fn test_parse_frontmatter_with_model() {
        let content = "---\nname: fast-review\ndescription: Quick review\nmodel: gpt-4o-mini\n---\n\n# Review\n";
        let meta = parse_frontmatter_str(content).unwrap();
        assert_eq!(meta.name, "fast-review");
        assert_eq!(meta.model.as_deref(), Some("gpt-4o-mini"));
    }

    #[test]
    fn test_parse_frontmatter_with_agent() {
        let content =
            "---\nname: deploy\ndescription: Deploy skill\nagent: devops\n---\n\n# Deploy\n";
        let meta = parse_frontmatter_str(content).unwrap();
        assert_eq!(meta.name, "deploy");
        assert_eq!(meta.agent.as_deref(), Some("devops"));
    }

    #[test]
    fn test_parse_frontmatter_no_agent_field() {
        let content = "---\nname: commit\ndescription: Git commit skill\n---\n\n# Commit\n";
        let meta = parse_frontmatter_str(content).unwrap();
        assert!(meta.agent.is_none());
    }

    #[test]
    fn test_parse_frontmatter_no_model_field() {
        let content = "---\nname: commit\ndescription: Git commit skill\n---\n\n# Commit\n";
        let meta = parse_frontmatter_str(content).unwrap();
        assert!(meta.model.is_none());
    }
}
