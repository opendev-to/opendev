//! YAML frontmatter parsing for custom agent markdown files.
//!
//! Pure functions that parse frontmatter delimiters, simple YAML key-value pairs,
//! list items, nested maps (permissions), and permission action strings.

use std::collections::HashMap;

use super::super::spec::{PermissionAction, PermissionRule};

/// Parsed frontmatter for a custom agent file.
#[derive(Debug, Default)]
pub(super) struct CustomAgentFrontmatter {
    pub description: Option<String>,
    pub mode: Option<String>,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub disabled: bool,
    pub hidden: bool,
    pub max_steps: Option<u32>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub color: Option<String>,
    pub permission: HashMap<String, PermissionRule>,
}

/// Split markdown content into optional YAML frontmatter and body.
///
/// Frontmatter is delimited by `---` lines at the start of the file.
pub(super) fn parse_frontmatter(content: &str) -> (Option<&str>, &str) {
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

/// Parse simple YAML frontmatter into a `CustomAgentFrontmatter`.
///
/// Handles: `key: value` pairs, `key:` followed by `  - item` lists,
/// and nested maps (for `permission`).
pub(super) fn parse_simple_yaml(yaml: &str) -> CustomAgentFrontmatter {
    let mut meta = CustomAgentFrontmatter::default();

    /// Tracks which top-level key we're inside for nested content.
    #[derive(PartialEq)]
    enum Context {
        None,
        Tools,
        Permission,
        /// Inside a permission tool entry (e.g., `bash:` under `permission:`).
        PermissionTool(String),
    }

    let mut ctx = Context::None;

    for line in yaml.lines() {
        let trimmed = line.trim();

        // Detect indentation level (number of leading spaces).
        let indent = line.len() - line.trim_start().len();

        // List item: "  - value" (indent >= 2)
        if let Some(item) = trimmed.strip_prefix("- ") {
            if ctx == Context::Tools {
                meta.tools.push(item.trim().to_string());
            }
            continue;
        }

        // Top-level key: no indentation
        if indent == 0 {
            if let Some((key, value)) = trimmed.split_once(':') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');

                ctx = Context::None;
                match key {
                    "description" => meta.description = Some(value.to_string()),
                    "mode" => meta.mode = Some(value.to_string()),
                    "model" => meta.model = Some(value.to_string()),
                    "disabled" | "disable" => meta.disabled = value == "true",
                    "hidden" => meta.hidden = value == "true",
                    "steps" | "max_steps" | "maxSteps" => {
                        meta.max_steps = value.parse().ok();
                    }
                    "max_tokens" | "maxTokens" => {
                        meta.max_tokens = value.parse().ok();
                    }
                    "temperature" => {
                        meta.temperature = value.parse().ok();
                    }
                    "top_p" | "topP" => {
                        meta.top_p = value.parse().ok();
                    }
                    "color" => meta.color = Some(value.to_string()),
                    "tools" => {
                        if value.is_empty() {
                            ctx = Context::Tools;
                        }
                    }
                    "permission" => {
                        if value.is_empty() {
                            ctx = Context::Permission;
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }

        // Indented content (indent >= 2): nested under current context
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim().trim_matches('"').trim_matches('\'');
            let value = value.trim().trim_matches('"').trim_matches('\'');

            match &ctx {
                Context::Permission => {
                    // `  bash: deny` → blanket action
                    // `  bash:` → start of pattern map
                    if value.is_empty() {
                        ctx = Context::PermissionTool(key.to_string());
                    } else if let Some(action) = parse_permission_action(value) {
                        meta.permission
                            .insert(key.to_string(), PermissionRule::Action(action));
                    }
                }
                Context::PermissionTool(tool_name) => {
                    // `    "git *": allow` → pattern rule
                    if let Some(action) = parse_permission_action(value) {
                        let entry = meta
                            .permission
                            .entry(tool_name.clone())
                            .or_insert_with(|| PermissionRule::Patterns(HashMap::new()));
                        if let PermissionRule::Patterns(patterns) = entry {
                            patterns.insert(key.to_string(), action);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    meta
}

/// Parse a permission action string ("allow", "deny", "ask").
fn parse_permission_action(s: &str) -> Option<PermissionAction> {
    match s {
        "allow" => Some(PermissionAction::Allow),
        "deny" => Some(PermissionAction::Deny),
        "ask" => Some(PermissionAction::Ask),
        _ => None,
    }
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
