//! Frontmatter and YAML parsing for skill files.
//!
//! Extracts metadata from YAML frontmatter blocks and provides
//! simple key-value YAML parsing without a full YAML library.
//! Supports scalar values, inline arrays, and block arrays.

use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use tracing::debug;

use super::metadata::{SkillContext, SkillEffort, SkillHookDef, SkillMetadata, SkillSource};

/// A value parsed from YAML frontmatter — either a scalar string or a list.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum FrontmatterValue {
    Scalar(String),
    List(Vec<String>),
}

impl FrontmatterValue {
    /// Get as a scalar string reference.
    pub(super) fn as_scalar(&self) -> Option<&str> {
        match self {
            Self::Scalar(s) => Some(s.as_str()),
            Self::List(_) => None,
        }
    }

    /// Get as a list, converting comma-separated scalars to a list.
    pub(super) fn as_list(&self) -> Vec<String> {
        match self {
            Self::Scalar(s) => {
                if s.is_empty() {
                    vec![]
                } else {
                    s.split(',')
                        .map(|p| p.trim().to_string())
                        .filter(|p| !p.is_empty())
                        .collect()
                }
            }
            Self::List(v) => v.clone(),
        }
    }

    /// Parse as boolean ("true"/"false"/"yes"/"no"/"1"/"0").
    pub(super) fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Scalar(s) => match s.trim().to_lowercase().as_str() {
                "true" | "yes" | "1" => Some(true),
                "false" | "no" | "0" => Some(false),
                _ => None,
            },
            Self::List(_) => None,
        }
    }
}

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
/// paths: ["**/*.rs", "**/*.ts"]
/// allowed-tools:
///   - Bash
///   - Edit
/// ---
/// ```
pub(super) fn parse_frontmatter_str(content: &str) -> Option<SkillMetadata> {
    let re = Regex::new(r"(?s)^---\r?\n(.*?)\r?\n---").ok()?;
    let caps = re.captures(content)?;
    let frontmatter = caps.get(1)?.as_str();

    let data = parse_simple_yaml(frontmatter);

    let name = data
        .get("name")
        .and_then(|v| v.as_scalar())
        .unwrap_or("")
        .to_string();
    let description = data
        .get("description")
        .and_then(|v| v.as_scalar())
        .unwrap_or("")
        .to_string();
    let description = if description.is_empty() {
        format!("Skill: {}", if name.is_empty() { "unknown" } else { &name })
    } else {
        description
    };
    let namespace = data
        .get("namespace")
        .and_then(|v| v.as_scalar())
        .unwrap_or("default")
        .to_string();

    let model = data
        .get("model")
        .and_then(|v| v.as_scalar())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let agent = data
        .get("agent")
        .and_then(|v| v.as_scalar())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let paths = data.get("paths").map(|v| v.as_list()).unwrap_or_default();

    let context = data
        .get("context")
        .and_then(|v| v.as_scalar())
        .and_then(SkillContext::from_str_opt)
        .unwrap_or_default();

    let effort = data
        .get("effort")
        .and_then(|v| v.as_scalar())
        .and_then(SkillEffort::from_str_opt)
        .unwrap_or_default();

    let allowed_tools = data
        .get("allowed-tools")
        .map(|v| v.as_list())
        .unwrap_or_default();

    let disable_model_invocation = data
        .get("disable-model-invocation")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let user_invocable = data
        .get("user-invocable")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let hooks = parse_hooks_from_frontmatter(&data);

    Some(SkillMetadata {
        name,
        description,
        namespace,
        path: None,
        source: SkillSource::Builtin,
        model,
        agent,
        paths,
        context,
        effort,
        allowed_tools,
        disable_model_invocation,
        user_invocable,
        hooks,
    })
}

/// Parse embedded hook definitions from frontmatter data.
///
/// Supports a `hooks` key with a list where each item has the format:
/// `"event:matcher:command"` or `"event:command"` (matcher empty).
fn parse_hooks_from_frontmatter(data: &HashMap<String, FrontmatterValue>) -> Vec<SkillHookDef> {
    let hooks_val = match data.get("hooks") {
        Some(v) => v,
        None => return vec![],
    };

    let items = hooks_val.as_list();
    let mut hooks = Vec::new();

    for item in &items {
        // Format: "event:matcher:command" or "event:command"
        let parts: Vec<&str> = item.splitn(3, ':').collect();
        if parts.len() >= 2 {
            let event = parts[0].trim().to_string();
            let (matcher, command) = if parts.len() == 3 {
                let m = parts[1].trim();
                let matcher = if m.is_empty() {
                    None
                } else {
                    Some(m.to_string())
                };
                (matcher, parts[2].trim().to_string())
            } else {
                (None, parts[1].trim().to_string())
            };

            if !event.is_empty() && !command.is_empty() {
                hooks.push(SkillHookDef {
                    event,
                    matcher,
                    command,
                });
            }
        }
    }

    hooks
}

/// Simple YAML-like key:value parser for frontmatter.
///
/// Handles flat `key: value` pairs, inline arrays `[a, b, c]`, and block
/// arrays (indented `- item` lines). Strips surrounding quotes from values.
pub(super) fn parse_simple_yaml(text: &str) -> HashMap<String, FrontmatterValue> {
    let mut result = HashMap::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        // Skip orphaned list items at the top level.
        if trimmed.starts_with("- ") {
            i += 1;
            continue;
        }

        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();

            if value.is_empty() {
                // Possible block list: check if next lines are "- item"
                let mut list_items = Vec::new();
                i += 1;
                while i < lines.len() {
                    let next = lines[i];
                    let next_trimmed = next.trim();
                    if let Some(rest) = next_trimmed.strip_prefix("- ") {
                        let item = strip_quotes(rest.trim());
                        list_items.push(item);
                        i += 1;
                    } else if next_trimmed.is_empty() {
                        i += 1;
                    } else {
                        break;
                    }
                }
                if list_items.is_empty() {
                    result.insert(key, FrontmatterValue::Scalar(String::new()));
                } else {
                    result.insert(key, FrontmatterValue::List(list_items));
                }
                continue;
            }

            // Inline array: ["a", "b", "c"]
            if value.starts_with('[') && value.ends_with(']') {
                let inner = &value[1..value.len() - 1];
                let items: Vec<String> = inner
                    .split(',')
                    .map(|s| strip_quotes(s.trim()))
                    .filter(|s| !s.is_empty())
                    .collect();
                result.insert(key, FrontmatterValue::List(items));
            } else {
                result.insert(key, FrontmatterValue::Scalar(strip_quotes(&value)));
            }
        }

        i += 1;
    }

    result
}

/// Strip surrounding quotes from a string value.
fn strip_quotes(s: &str) -> String {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
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
#[path = "parsing_tests.rs"]
mod tests;
