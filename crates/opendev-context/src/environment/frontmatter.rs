//! YAML frontmatter parsing and HTML comment stripping for instruction files.
//!
//! Rule files in `.opendev/rules/` can have YAML frontmatter with a `paths` field
//! that makes them conditional. HTML block comments (`<!-- ... -->`) are stripped
//! from instruction content while preserving comments inside fenced code blocks.

use regex::Regex;
use serde::Deserialize;

/// Parsed YAML frontmatter from a rule file.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Frontmatter {
    /// Glob patterns that make this rule conditional.
    /// When present, the rule only applies to files matching at least one pattern.
    #[serde(default)]
    pub paths: Option<Vec<String>>,
}

/// Parse YAML frontmatter from instruction content.
///
/// Frontmatter is delimited by `---` lines at the start of the file.
/// Returns `(parsed_frontmatter, remaining_content)`.
pub fn parse_frontmatter(content: &str) -> (Option<Frontmatter>, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (None, content.to_string());
    }

    // Find the opening delimiter line end
    let after_open = match trimmed.find('\n') {
        Some(i) => i + 1,
        None => return (None, content.to_string()),
    };

    // Find the closing `---` delimiter
    let rest = &trimmed[after_open..];
    let close_pos = rest
        .lines()
        .enumerate()
        .find(|(_, line)| line.trim() == "---")
        .map(|(i, _)| {
            // Calculate byte offset of the closing delimiter line
            rest.lines()
                .take(i)
                .map(|l| l.len() + 1) // +1 for newline
                .sum::<usize>()
        });

    let close_pos = match close_pos {
        Some(p) => p,
        None => return (None, content.to_string()),
    };

    let yaml_str = &rest[..close_pos];
    let remaining = &rest[close_pos..];
    // Skip the closing `---` line itself
    let remaining = match remaining.find('\n') {
        Some(i) => &remaining[i + 1..],
        None => "",
    };

    let frontmatter: Option<Frontmatter> = serde_yaml::from_str(yaml_str).ok();

    (frontmatter, remaining.to_string())
}

/// Strip block-level HTML comments from instruction content.
///
/// Removes `<!-- ... -->` patterns from content while preserving
/// comments inside fenced code blocks (` ``` `).
pub fn strip_html_comments(content: &str) -> String {
    let comment_re = Regex::new(r"<!--[\s\S]*?-->").expect("valid regex");

    let mut result = String::with_capacity(content.len());
    let mut in_code_block = false;
    let mut current_block = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code_block {
                // Closing code block — flush block content as-is
                current_block.push_str(line);
                current_block.push('\n');
                result.push_str(&current_block);
                current_block.clear();
                in_code_block = false;
            } else {
                // Opening code block — flush any pending non-code content
                if !current_block.is_empty() {
                    let stripped = comment_re.replace_all(&current_block, "");
                    result.push_str(&stripped);
                    current_block.clear();
                }
                in_code_block = true;
                current_block.push_str(line);
                current_block.push('\n');
            }
        } else {
            current_block.push_str(line);
            current_block.push('\n');
        }
    }

    // Flush remaining content
    if !current_block.is_empty() {
        if in_code_block {
            // Unclosed code block — preserve as-is
            result.push_str(&current_block);
        } else {
            let stripped = comment_re.replace_all(&current_block, "");
            result.push_str(&stripped);
        }
    }

    // Remove trailing newline added by our line iteration if original didn't have one
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

#[cfg(test)]
#[path = "frontmatter_tests.rs"]
mod tests;
