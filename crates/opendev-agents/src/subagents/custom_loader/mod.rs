//! Custom agent loading from markdown files.
//!
//! Loads user-defined subagent specs from `.opendev/agents/*.md` files.
//! Each file uses simple YAML frontmatter for metadata and the body as the system prompt.

mod parser;

use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use super::spec::SubAgentSpec;
use parser::{CustomAgentFrontmatter, parse_frontmatter, parse_simple_yaml};

/// Load custom agent specs from the given directories.
///
/// Recursively scans each directory for `*.md` files (supporting nested
/// subdirectories like `agents/review/deep.md`), parses YAML frontmatter
/// and body, and returns a list of `SubAgentSpec`.
/// Directories that don't exist are silently skipped.
pub fn load_custom_agents(dirs: &[PathBuf]) -> Vec<SubAgentSpec> {
    let mut specs = Vec::new();

    for dir in dirs {
        if !dir.is_dir() {
            debug!(dir = %dir.display(), "Custom agents directory does not exist, skipping");
            continue;
        }

        let md_files = collect_md_files_recursive(dir);
        for path in md_files {
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

/// Recursively collect all `*.md` files from a directory.
fn collect_md_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            warn!(dir = %dir.display(), error = %e, "Failed to read directory");
            return files;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_md_files_recursive(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            files.push(path);
        }
    }

    files.sort();
    files
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

    if meta.hidden {
        spec = spec.with_hidden(true);
    }

    if let Some(steps) = meta.max_steps {
        spec = spec.with_max_steps(steps);
    }

    if let Some(max_tokens) = meta.max_tokens {
        spec = spec.with_max_tokens(max_tokens);
    }

    if let Some(temp) = meta.temperature {
        spec = spec.with_temperature(temp);
    }

    if let Some(top_p) = meta.top_p {
        spec = spec.with_top_p(top_p);
    }

    if let Some(ref mode_str) = meta.mode {
        spec = spec.with_mode(super::spec::AgentMode::parse_mode(mode_str));
    }

    if let Some(color) = meta.color {
        spec = spec.with_color(color);
    }

    if !meta.permission.is_empty() {
        spec = spec.with_permission(meta.permission);
    }

    Ok(Some(spec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subagents::spec::PermissionAction;

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

    #[test]
    fn test_load_with_extended_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("custom.md"),
            "---\ndescription: Custom agent\nhidden: true\nsteps: 50\ntemperature: 0.3\n---\nYou are custom.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert!(specs[0].hidden);
        assert_eq!(specs[0].max_steps, Some(50));
        assert_eq!(specs[0].temperature, Some(0.3));
    }

    #[test]
    fn test_load_with_top_p() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("precise.md"),
            "---\ndescription: Precise agent\ntop_p: 0.9\n---\nPrecise agent.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].top_p, Some(0.9));
    }

    #[test]
    fn test_load_with_mode_all() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("versatile.md"),
            "---\ndescription: Versatile\nmode: all\n---\nVersatile agent.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].mode, crate::subagents::AgentMode::All);
    }

    #[test]
    fn test_load_with_color() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("colorful.md"),
            "---\ndescription: Colorful agent\ncolor: \"#38A3EE\"\n---\nColorful agent.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].color.as_deref(), Some("#38A3EE"));
    }

    #[test]
    fn test_load_with_max_tokens() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("verbose.md"),
            "---\ndescription: Verbose agent\nmax_tokens: 8192\n---\nVerbose agent.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].max_tokens, Some(8192));
    }

    #[test]
    fn test_load_with_max_tokens_camel_case() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("verbose2.md"),
            "---\nmaxTokens: 16384\n---\nVerbose agent.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].max_tokens, Some(16384));
    }

    #[test]
    fn test_load_with_max_steps_alias() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("agent.md"),
            "---\nmaxSteps: 100\n---\nAgent body.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].max_steps, Some(100));
    }

    #[test]
    fn test_load_recursive_nested_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        let nested = agent_dir.join("review");
        std::fs::create_dir_all(&nested).unwrap();

        // Top-level agent
        std::fs::write(
            agent_dir.join("top.md"),
            "---\ndescription: Top agent\n---\nTop agent prompt.",
        )
        .unwrap();

        // Nested agent
        std::fs::write(
            nested.join("deep.md"),
            "---\ndescription: Deep agent\n---\nDeep agent prompt.",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 2);
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"top"));
        assert!(names.contains(&"deep"));
    }

    #[test]
    fn test_load_agent_with_permission() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("restricted.md"),
            "---\ndescription: Restricted agent\npermission:\n  edit: deny\n  bash: ask\n---\n\nRestricted agent.\n",
        )
        .unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(
            specs[0].evaluate_permission("edit", "any_file"),
            Some(PermissionAction::Deny)
        );
        assert_eq!(
            specs[0].evaluate_permission("bash", "any_command"),
            Some(PermissionAction::Ask)
        );
    }

    #[test]
    fn test_load_recursive_skips_non_md() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join("agents");
        let nested = agent_dir.join("sub");
        std::fs::create_dir_all(&nested).unwrap();

        std::fs::write(
            agent_dir.join("valid.md"),
            "---\ndescription: Valid\n---\nValid.",
        )
        .unwrap();
        std::fs::write(nested.join("config.json"), r#"{"key": "val"}"#).unwrap();
        std::fs::write(nested.join("notes.txt"), "not an agent").unwrap();

        let specs = load_custom_agents(&[agent_dir]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "valid");
    }
}
