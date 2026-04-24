//! Fine-grained permission rule set with glob-based matching and directory scoping.
//!
//! Provides [`PermissionRuleSet`] for ordered, priority-based permission evaluation,
//! with optional per-directory scoping via glob patterns.

mod glob;

pub use glob::{glob_matches, glob_matches_path};

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Action to take when a permission rule matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    /// Silently allow the operation.
    Allow,
    /// Silently deny the operation.
    Deny,
    /// Prompt the user for confirmation.
    Prompt,
}

/// A single permission rule with glob pattern matching and optional directory scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// Glob pattern matched against `"tool_name:args"` (e.g. `"bash:rm *"`, `"edit:*"`).
    pub pattern: String,
    /// What to do when the pattern matches.
    pub action: PermissionAction,
    /// Higher-priority rules are evaluated first.
    pub priority: i32,
    /// Optional glob restricting the rule to operations within matching directories.
    /// Example: `Some("src/**")` only applies when the working directory is under `src/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory_scope: Option<String>,
}

/// An ordered collection of permission rules evaluated highest-priority-first.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionRuleSet {
    rules: Vec<PermissionRule>,
}

/// Check whether a file path points to a sensitive file that should be denied by default.
///
/// Returns `true` for `.env`, `.env.*` (but NOT `.env.example`), and other credential files.
pub fn is_sensitive_file(path: &str) -> bool {
    let filename = path.rsplit('/').next().unwrap_or(path);
    let lower = filename.to_lowercase();

    // .env and .env.* (but allow .env.example, .env.sample, .env.template)
    if lower == ".env" {
        return true;
    }
    if let Some(suffix) = lower.strip_prefix(".env.") {
        return !matches!(suffix, "example" | "sample" | "template");
    }

    // Other common credential files
    matches!(
        lower.as_str(),
        "credentials.json"
            | "service-account.json"
            | "id_rsa"
            | "id_ed25519"
            | ".npmrc"
            | ".pypirc"
    )
}

impl PermissionRuleSet {
    /// Create an empty rule set.
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Create a rule set with built-in security defaults.
    ///
    /// Includes auto-deny for reading/writing `.env` files and other credential files,
    /// while allowing `.env.example`.
    pub fn with_defaults() -> Self {
        let mut rs = Self::new();

        // Deny reading sensitive env files (high priority)
        rs.add_rule(PermissionRule {
            pattern: "read_file:*.env".into(),
            action: PermissionAction::Deny,
            priority: 1000,
            directory_scope: None,
        });
        rs.add_rule(PermissionRule {
            pattern: "read_file:*.env.*".into(),
            action: PermissionAction::Deny,
            priority: 1000,
            directory_scope: None,
        });
        // Allow .env.example specifically (higher priority overrides deny)
        rs.add_rule(PermissionRule {
            pattern: "read_file:*.env.example".into(),
            action: PermissionAction::Allow,
            priority: 1001,
            directory_scope: None,
        });
        rs.add_rule(PermissionRule {
            pattern: "read_file:*.env.sample".into(),
            action: PermissionAction::Allow,
            priority: 1001,
            directory_scope: None,
        });
        rs.add_rule(PermissionRule {
            pattern: "read_file:*.env.template".into(),
            action: PermissionAction::Allow,
            priority: 1001,
            directory_scope: None,
        });

        // Deny editing/writing sensitive env files
        rs.add_rule(PermissionRule {
            pattern: "edit_file:*.env".into(),
            action: PermissionAction::Deny,
            priority: 1000,
            directory_scope: None,
        });
        rs.add_rule(PermissionRule {
            pattern: "edit_file:*.env.*".into(),
            action: PermissionAction::Deny,
            priority: 1000,
            directory_scope: None,
        });
        rs.add_rule(PermissionRule {
            pattern: "write_file:*.env".into(),
            action: PermissionAction::Deny,
            priority: 1000,
            directory_scope: None,
        });
        rs.add_rule(PermissionRule {
            pattern: "write_file:*.env.*".into(),
            action: PermissionAction::Deny,
            priority: 1000,
            directory_scope: None,
        });

        rs
    }

    /// Add a rule to the set.
    pub fn add_rule(&mut self, rule: PermissionRule) {
        self.rules.push(rule);
    }

    /// Remove all rules matching a predicate.
    pub fn remove_rules<F: Fn(&PermissionRule) -> bool>(&mut self, predicate: F) {
        self.rules.retain(|r| !predicate(r));
    }

    /// Read-only access to the rules.
    pub fn rules(&self) -> &[PermissionRule] {
        &self.rules
    }

    /// Evaluate a tool invocation against the rule set.
    ///
    /// `tool_name` is the tool being invoked (e.g. `"bash"`, `"edit"`).
    /// `args` is the argument string (e.g. the command or file path).
    /// `working_dir` is the optional directory context for directory-scoped rules.
    ///
    /// Returns the action from the highest-priority matching rule, or `None` if
    /// no rule matches.
    pub fn evaluate(
        &self,
        tool_name: &str,
        args: &str,
        working_dir: Option<&Path>,
    ) -> Option<PermissionAction> {
        let input = format!("{tool_name}:{args}");

        let mut sorted: Vec<&PermissionRule> = self.rules.iter().collect();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.priority));

        for rule in sorted {
            // Check directory scope first
            if let Some(ref scope) = rule.directory_scope {
                match working_dir {
                    Some(dir) => {
                        if !glob_matches_path(scope, &dir.to_string_lossy()) {
                            continue;
                        }
                    }
                    None => continue, // scoped rule requires a directory
                }
            }

            if glob_matches(&rule.pattern, &input) {
                return Some(rule.action.clone());
            }
        }

        None
    }

    /// Convenience wrapper without directory context.
    pub fn evaluate_simple(&self, tool_name: &str, args: &str) -> Option<PermissionAction> {
        self.evaluate(tool_name, args, None)
    }
}

#[cfg(test)]
mod tests;
