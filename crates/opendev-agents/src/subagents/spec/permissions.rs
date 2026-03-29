use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Action to take when a permission rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    /// Allow the tool call without user approval.
    Allow,
    /// Deny the tool call entirely.
    Deny,
    /// Prompt the user for approval.
    Ask,
}

/// A permission rule for a tool — either a blanket action or pattern-specific.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionRule {
    /// Single action applies to all patterns for this tool.
    Action(PermissionAction),
    /// Map of glob patterns to actions.
    /// Example: `{ "*": "ask", "git *": "allow", "rm -rf *": "deny" }`
    Patterns(HashMap<String, PermissionAction>),
}

/// Compute specificity of a glob pattern (higher = more specific).
///
/// `"*"` → 0, `"git *"` → 4, `"git status"` → 10 (exact match).
/// Patterns with fewer wildcards and more literal characters are more specific.
pub(crate) fn pattern_specificity(pattern: &str) -> usize {
    if pattern == "*" {
        return 0;
    }
    // Count non-wildcard characters as specificity score.
    pattern.chars().filter(|c| *c != '*' && *c != '?').count()
}

/// Simple glob-style matching: `*` matches any sequence, `?` matches one char.
///
/// Matching is case-sensitive and operates on the full string.
pub(crate) fn glob_match(pattern: &str, input: &str) -> bool {
    let pattern = pattern.as_bytes();
    let input = input.as_bytes();
    let mut pi = 0;
    let mut ii = 0;
    let mut star_pi = usize::MAX;
    let mut star_ii = 0;

    while ii < input.len() {
        if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == input[ii]) {
            pi += 1;
            ii += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star_pi = pi;
            star_ii = ii;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ii += 1;
            ii = star_ii;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

#[cfg(test)]
#[path = "permissions_tests.rs"]
mod tests;
