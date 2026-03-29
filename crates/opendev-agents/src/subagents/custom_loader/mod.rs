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
mod tests;
