//! Template expansion: placeholder substitution, shell execution, argument replacement.

use std::collections::HashMap;

use regex::Regex;

use super::CustomCommand;

/// Parse YAML frontmatter delimited by `---` lines.
///
/// Returns `(frontmatter_map, body)` where frontmatter_map contains
/// key-value pairs and body is the content after the closing `---`.
/// If no frontmatter is present, returns empty map and full content.
pub(super) fn parse_frontmatter(content: &str) -> (HashMap<String, String>, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (HashMap::new(), content.to_string());
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let after_first = after_first.trim_start_matches(['\r', '\n']);

    if let Some(end_pos) = after_first.find("\n---") {
        let fm_text = &after_first[..end_pos];
        let body_start = end_pos + 4; // skip \n---
        let body = after_first[body_start..].trim_start_matches(['\r', '\n']);

        let mut map = HashMap::new();
        for line in fm_text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_string();
                let value = value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                map.insert(key, value);
            }
        }

        (map, body.to_string())
    } else {
        // No closing ---, treat as no frontmatter
        (HashMap::new(), content.to_string())
    }
}

/// Execute shell command substitutions in the template.
///
/// Replaces `!`cmd`` patterns with the stdout of the command.
/// Failures are replaced with `[error: ...]` inline.
pub(super) fn expand_shell_commands(content: &str) -> String {
    let re = Regex::new(r"!`([^`]+)`").expect("valid regex");
    re.replace_all(content, |caps: &regex::Captures| {
        let cmd = &caps[1];
        match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                format!("[error: {cmd}: {}]", stderr.trim())
            }
            Err(e) => format!("[error: {cmd}: {e}]"),
        }
    })
    .to_string()
}

impl CustomCommand {
    /// Expand the template with the given arguments and optional context.
    ///
    /// - Executes `!`cmd`` shell substitutions.
    /// - Replaces `$ARGUMENTS` with the full argument string.
    /// - Replaces `$1`, `$2`, etc. with positional args (whitespace-split).
    /// - Cleans up unreplaced `$N` positional patterns.
    /// - Replaces context variables: `$KEY` → value.
    pub fn expand(&self, arguments: &str, context: Option<&HashMap<String, String>>) -> String {
        let mut result = self.template.replace("$ARGUMENTS", arguments);

        // Replace positional $1, $2, etc.
        let parts: Vec<&str> = if arguments.is_empty() {
            Vec::new()
        } else {
            arguments.split_whitespace().collect()
        };
        for (i, part) in parts.iter().enumerate() {
            let placeholder = format!("${}", i + 1);
            result = result.replace(&placeholder, part);
        }

        // Clean up unreplaced positional args
        let re = Regex::new(r"\$\d+").expect("valid regex");
        result = re.replace_all(&result, "").to_string();

        // Replace context variables
        if let Some(ctx) = context {
            for (key, value) in ctx {
                let placeholder = format!("${}", key.to_uppercase());
                result = result.replace(&placeholder, value);
            }
        }

        // Execute shell command substitutions last
        result = expand_shell_commands(&result);

        result.trim().to_string()
    }
}

#[cfg(test)]
#[path = "expansion_tests.rs"]
mod tests;
