//! Custom agent loading from markdown files.
//!
//! Loads user-defined subagent specs from `.opendev/agents/*.md` files.
//! Each file uses simple YAML frontmatter for metadata and the body as the system prompt.

use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use super::spec::SubAgentSpec;

/// Parsed frontmatter for a custom agent file.
#[derive(Debug, Default)]
struct CustomAgentFrontmatter {
    description: Option<String>,
    mode: Option<String>,
    model: Option<String>,
    tools: Vec<String>,
    disabled: bool,
}

/// Load custom agent specs from the given directories.
///
/// Scans each directory for `*.md` files, parses YAML frontmatter and body,
/// and returns a list of `SubAgentSpec`. Directories that don't exist are silently skipped.
pub fn load_custom_agents(dirs: &[PathBuf]) -> Vec<SubAgentSpec> {
    let mut specs = Vec::new();

    for dir in dirs {
        if !dir.is_dir() {
            debug!(dir = %dir.display(), "Custom agents directory does not exist, skipping");
            continue;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(dir = %dir.display(), error = %e, "Failed to read custom agents directory");
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            match load_agent_file(&path) {
                Ok(Some(spec)) => {
                    debug!(name = %spec.name, path = %path.display(), "Loaded custom agent");
                    specs.push(spec);
                }
                Ok(None) => {
                    debug!(path = %path.display(), "Skipped custom agent (disabled or wrong mode)");
                }
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to parse custom agent file");
                }
            }
        }
    }

    specs
}

/// Parse a single agent markdown file into a SubAgentSpec.
///
/// Returns `Ok(None)` if the agent is disabled or has an incompatible mode.
fn load_agent_file(path: &Path) -> Result<Option<SubAgentSpec>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))?;

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let (frontmatter_str, body) = parse_frontmatter(&content);

    let meta = if let Some(fm) = frontmatter_str {
        parse_simple_yaml(fm)
    } else {
        CustomAgentFrontmatter::default()
    };

    // Skip disabled agents
    if meta.disabled {
        return Ok(None);
    }

    // Only load agents with subagent-compatible mode
    let mode = meta.mode.as_deref().unwrap_or("subagent");
    if mode != "subagent" && mode != "all" {
        return Ok(None);
    }

    let description = meta
        .description
        .unwrap_or_else(|| format!("Custom agent: {name}"));

    let mut spec = SubAgentSpec::new(name, description, body.trim());

    if !meta.tools.is_empty() {
        spec = spec.with_tools(meta.tools);
    }

    if let Some(model) = meta.model {
        spec = spec.with_model(model);
    }

    Ok(Some(spec))
}

/// Parse simple YAML frontmatter into a `CustomAgentFrontmatter`.
///
/// Handles: `key: value` pairs and `key:` followed by `  - item` lists.
fn parse_simple_yaml(yaml: &str) -> CustomAgentFrontmatter {
    let mut meta = CustomAgentFrontmatter::default();
    let mut current_list_key: Option<&str> = None;

    for line in yaml.lines() {
        let trimmed = line.trim();

        // List item: "  - value"
        if let Some(item) = trimmed.strip_prefix("- ") {
            if current_list_key == Some("tools") {
                meta.tools.push(item.trim().to_string());
            }
            continue;
        }

        // Key-value pair
        current_list_key = None;
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');

            match key {
                "description" => meta.description = Some(value.to_string()),
                "mode" => meta.mode = Some(value.to_string()),
                "model" => meta.model = Some(value.to_string()),
                "disabled" => meta.disabled = value == "true",
                "tools" => {
                    if value.is_empty() {
                        current_list_key = Some("tools");
                    }
                }
                _ => {}
            }
        }
    }

    meta
}

/// Split markdown content into optional YAML frontmatter and body.
///
/// Frontmatter is delimited by `---` lines at the start of the file.
fn parse_frontmatter(content: &str) -> (Option<&str>, &str) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (None, content);
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    if let Some(end_pos) = after_first.find("\n---") {
        let fm = after_first[..end_pos].trim();
        let body_start = end_pos + 4; // skip \n---
        let body = after_first[body_start..].trim_start_matches('\n');
        (Some(fm), body)
    } else {
        // No closing ---, treat entire content as body
        (None, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = "---\ndescription: test\n---\nBody here.";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm, Some("description: test"));
        assert_eq!(body, "Body here.");
    }

    #[test]
    fn test_parse_frontmatter_none() {
        let content = "Just a body with no frontmatter.";
        let (fm, body) = parse_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_parse_frontmatter_no_closing() {
        let content = "---\ndescription: test\nNo closing delimiter.";
        let (fm, body) = parse_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_parse_simple_yaml() {
        let yaml = "description: \"Reviews code\"\ntools:\n  - read_file\n  - search";
        let meta = parse_simple_yaml(yaml);
        assert_eq!(meta.description.as_deref(), Some("Reviews code"));
        assert_eq!(meta.tools, vec!["read_file", "search"]);
    }

    #[test]
    fn test_parse_simple_yaml_disabled() {
        let yaml = "disabled: true\nmodel: gpt-4o";
        let meta = parse_simple_yaml(yaml);
        assert!(meta.disabled);
        assert_eq!(meta.model.as_deref(), Some("gpt-4o"));
    }

    #[test]
    fn test_load_custom_agent_md() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("my-reviewer.md"),
            "---\ndescription: \"Reviews code\"\ntools:\n  - read_file\n  - search\n---\n\nYou are a code reviewer.\n",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "my-reviewer");
        assert!(specs[0].tools.contains(&"read_file".to_string()));
        assert!(specs[0].tools.contains(&"search".to_string()));
        assert!(specs[0].system_prompt.contains("code reviewer"));
    }

    #[test]
    fn test_load_disabled_agent() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("disabled.md"),
            "---\ndisabled: true\n---\nShould not load.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert!(specs.is_empty());
    }

    #[test]
    fn test_load_primary_mode_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("primary-only.md"),
            "---\nmode: primary\n---\nPrimary mode agent.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert!(specs.is_empty());
    }

    #[test]
    fn test_load_no_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(agent_dir.join("simple.md"), "You are a simple agent.").unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "simple");
        assert!(specs[0].system_prompt.contains("simple agent"));
        assert!(!specs[0].has_tool_restriction());
    }

    #[test]
    fn test_load_nonexistent_dir() {
        let specs = load_custom_agents(&[PathBuf::from("/nonexistent/path/agents")]);
        assert!(specs.is_empty());
    }

    #[test]
    fn test_load_with_model_override() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("fast.md"),
            "---\nmodel: gpt-4o-mini\n---\nFast agent.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].model.as_deref(), Some("gpt-4o-mini"));
    }
}
