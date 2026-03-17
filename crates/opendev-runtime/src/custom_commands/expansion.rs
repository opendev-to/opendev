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
mod tests {
    use super::*;

    fn make_command(template: &str) -> CustomCommand {
        CustomCommand {
            name: "test".to_string(),
            template: template.to_string(),
            source: "test:test.md".to_string(),
            description: "Test command".to_string(),
            model: None,
            agent: None,
            subtask: false,
        }
    }

    // ── Expansion tests ──

    #[test]
    fn test_expand_arguments() {
        let cmd = make_command("Review this: $ARGUMENTS\nDone.");
        let result = cmd.expand("auth module", None);
        assert_eq!(result, "Review this: auth module\nDone.");
    }

    #[test]
    fn test_expand_positional_args() {
        let cmd = make_command("Fix $1 in $2");
        let result = cmd.expand("bug main.rs", None);
        assert_eq!(result, "Fix bug in main.rs");
    }

    #[test]
    fn test_expand_unreplaced_positional_cleaned() {
        let cmd = make_command("Use $1 and $2 and $3");
        let result = cmd.expand("foo bar", None);
        assert_eq!(result, "Use foo and bar and");
    }

    #[test]
    fn test_expand_empty_arguments() {
        let cmd = make_command("Hello $ARGUMENTS world");
        let result = cmd.expand("", None);
        assert_eq!(result, "Hello  world");
    }

    #[test]
    fn test_expand_context_variables() {
        let cmd = make_command("Check $FILE for $LANG issues");
        let mut ctx = HashMap::new();
        ctx.insert("file".to_string(), "main.rs".to_string());
        ctx.insert("lang".to_string(), "Rust".to_string());
        let result = cmd.expand("", Some(&ctx));
        assert_eq!(result, "Check main.rs for Rust issues");
    }

    #[test]
    fn test_expand_combined() {
        let cmd = make_command("Review $ARGUMENTS in $FILE focusing on $1");
        let mut ctx = HashMap::new();
        ctx.insert("file".to_string(), "lib.rs".to_string());
        let result = cmd.expand("security perf", Some(&ctx));
        assert_eq!(
            result,
            "Review security perf in lib.rs focusing on security"
        );
    }

    // ── Shell substitution tests ──

    #[test]
    fn test_expand_shell_command() {
        let cmd = make_command("Hello !`echo world`");
        let result = cmd.expand("", None);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_expand_shell_command_with_args() {
        let cmd = make_command("Version: !`echo 1.2.3`");
        let result = cmd.expand("", None);
        assert_eq!(result, "Version: 1.2.3");
    }

    #[test]
    fn test_expand_shell_command_failure() {
        let cmd = make_command("Result: !`false`");
        let result = cmd.expand("", None);
        assert!(result.starts_with("Result: [error:"));
    }

    #[test]
    fn test_expand_shell_no_substitution() {
        let cmd = make_command("No shell here");
        let result = cmd.expand("", None);
        assert_eq!(result, "No shell here");
    }

    // ── Frontmatter parsing tests ──

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = "---\ndescription: Code review\nmodel: gpt-4o\n---\n\nReview $ARGUMENTS";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.get("description").unwrap(), "Code review");
        assert_eq!(fm.get("model").unwrap(), "gpt-4o");
        assert_eq!(body.trim(), "Review $ARGUMENTS");
    }

    #[test]
    fn test_parse_frontmatter_quoted_values() {
        let content = "---\ndescription: \"Commit and push\"\nagent: 'reviewer'\n---\nBody";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.get("description").unwrap(), "Commit and push");
        assert_eq!(fm.get("agent").unwrap(), "reviewer");
        assert_eq!(body.trim(), "Body");
    }

    #[test]
    fn test_parse_frontmatter_none() {
        let content = "# No frontmatter\nJust a template";
        let (fm, body) = parse_frontmatter(content);
        assert!(fm.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn test_parse_frontmatter_no_closing() {
        let content = "---\nkey: value\nno closing delimiter";
        let (fm, body) = parse_frontmatter(content);
        assert!(fm.is_empty());
        assert_eq!(body, content);
    }
}
