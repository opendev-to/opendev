//! Fine-grained permission rule set with glob-based matching and directory scoping.
//!
//! Provides [`PermissionRuleSet`] for ordered, priority-based permission evaluation,
//! with optional per-directory scoping via glob patterns.

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

impl PermissionRuleSet {
    /// Create an empty rule set.
    pub fn new() -> Self {
        Self { rules: Vec::new() }
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
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        for rule in sorted {
            // Check directory scope first
            if let Some(ref scope) = rule.directory_scope {
                match working_dir {
                    Some(dir) => {
                        if !glob_matches(scope, &dir.to_string_lossy()) {
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

/// Simple glob matching supporting `*` (any chars except `/`) and `**` (any chars including `/`).
///
/// The pattern is anchored: it must match the entire input string.
pub fn glob_matches(pattern: &str, input: &str) -> bool {
    glob_matches_inner(pattern.as_bytes(), input.as_bytes())
}

fn glob_matches_inner(pattern: &[u8], input: &[u8]) -> bool {
    let mut pi = 0;
    let mut ii = 0;
    let mut star_pi = usize::MAX;
    let mut star_ii = 0;
    // Track `**` separately since it matches `/`
    let mut dstar_pi = usize::MAX;
    let mut dstar_ii = 0;

    while ii < input.len() {
        if pi + 1 < pattern.len() && pattern[pi] == b'*' && pattern[pi + 1] == b'*' {
            // `**` — matches everything including `/`
            dstar_pi = pi;
            dstar_ii = ii;
            pi += 2;
            // Skip trailing `/` after `**`
            if pi < pattern.len() && pattern[pi] == b'/' {
                pi += 1;
            }
            continue;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            // `*` — matches everything except `/`
            star_pi = pi;
            star_ii = ii;
            pi += 1;
            continue;
        } else if pi < pattern.len() && (pattern[pi] == input[ii] || pattern[pi] == b'?') {
            pi += 1;
            ii += 1;
            continue;
        }

        // Backtrack to single `*`
        if star_pi != usize::MAX && input[star_ii] != b'/' {
            star_ii += 1;
            ii = star_ii;
            pi = star_pi + 1;
            continue;
        }

        // Backtrack to `**`
        if dstar_pi != usize::MAX {
            dstar_ii += 1;
            ii = dstar_ii;
            pi = dstar_pi + 2;
            if pi < pattern.len() && pattern[pi] == b'/' {
                pi += 1;
            }
            continue;
        }

        return false;
    }

    // Consume trailing `*` or `**`
    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ---- glob matching tests ----

    #[test]
    fn test_glob_exact_match() {
        assert!(glob_matches("hello", "hello"));
        assert!(!glob_matches("hello", "world"));
    }

    #[test]
    fn test_glob_star() {
        assert!(glob_matches("bash:*", "bash:ls -la"));
        assert!(glob_matches("edit:*", "edit:foo.rs"));
        assert!(!glob_matches("bash:*", "edit:foo.rs"));
    }

    #[test]
    fn test_glob_double_star() {
        assert!(glob_matches("src/**", "src/foo/bar/baz.rs"));
        assert!(glob_matches("**/*.rs", "src/foo/bar/baz.rs"));
        assert!(!glob_matches("src/**", "vendor/foo.rs"));
    }

    #[test]
    fn test_glob_question_mark() {
        assert!(glob_matches("ba?h:*", "bash:cmd"));
        assert!(!glob_matches("ba?h:*", "batch:cmd"));
    }

    #[test]
    fn test_glob_star_no_slash() {
        // Single `*` should not match `/`
        assert!(!glob_matches("src/*", "src/foo/bar.rs"));
        assert!(glob_matches("src/*", "src/bar.rs"));
    }

    // ---- PermissionRuleSet tests ----

    #[test]
    fn test_ruleset_empty_returns_none() {
        let rs = PermissionRuleSet::new();
        assert_eq!(rs.evaluate_simple("bash", "ls"), None);
    }

    #[test]
    fn test_ruleset_basic_allow() {
        let mut rs = PermissionRuleSet::new();
        rs.add_rule(PermissionRule {
            pattern: "bash:*".into(),
            action: PermissionAction::Allow,
            priority: 10,
            directory_scope: None,
        });
        assert_eq!(
            rs.evaluate_simple("bash", "ls -la"),
            Some(PermissionAction::Allow)
        );
        assert_eq!(rs.evaluate_simple("edit", "foo.rs"), None);
    }

    #[test]
    fn test_ruleset_priority_ordering() {
        let mut rs = PermissionRuleSet::new();
        rs.add_rule(PermissionRule {
            pattern: "bash:*".into(),
            action: PermissionAction::Allow,
            priority: 1,
            directory_scope: None,
        });
        rs.add_rule(PermissionRule {
            pattern: "bash:rm *".into(),
            action: PermissionAction::Deny,
            priority: 10,
            directory_scope: None,
        });
        // "rm -rf /" matches the Deny rule with higher priority
        assert_eq!(
            rs.evaluate_simple("bash", "rm -rf /"),
            Some(PermissionAction::Deny)
        );
        // "ls" only matches the Allow rule
        assert_eq!(
            rs.evaluate_simple("bash", "ls"),
            Some(PermissionAction::Allow)
        );
    }

    #[test]
    fn test_ruleset_directory_scope() {
        let mut rs = PermissionRuleSet::new();
        rs.add_rule(PermissionRule {
            pattern: "edit:*".into(),
            action: PermissionAction::Allow,
            priority: 10,
            directory_scope: Some("src/**".into()),
        });
        rs.add_rule(PermissionRule {
            pattern: "edit:*".into(),
            action: PermissionAction::Deny,
            priority: 10,
            directory_scope: Some("vendor/**".into()),
        });

        let src_dir = PathBuf::from("src/components/button.rs");
        let vendor_dir = PathBuf::from("vendor/lib/foo.rs");

        assert_eq!(
            rs.evaluate("edit", "foo.rs", Some(&src_dir)),
            Some(PermissionAction::Allow)
        );
        assert_eq!(
            rs.evaluate("edit", "foo.rs", Some(&vendor_dir)),
            Some(PermissionAction::Deny)
        );
        // No directory => scoped rules don't match
        assert_eq!(rs.evaluate("edit", "foo.rs", None), None);
    }

    #[test]
    fn test_ruleset_scoped_and_unscoped_mix() {
        let mut rs = PermissionRuleSet::new();
        // Low-priority blanket allow
        rs.add_rule(PermissionRule {
            pattern: "edit:*".into(),
            action: PermissionAction::Allow,
            priority: 1,
            directory_scope: None,
        });
        // High-priority deny for vendor
        rs.add_rule(PermissionRule {
            pattern: "edit:*".into(),
            action: PermissionAction::Deny,
            priority: 100,
            directory_scope: Some("vendor/**".into()),
        });

        let vendor = PathBuf::from("vendor/lib.rs");
        let src = PathBuf::from("src/main.rs");

        // vendor => Deny wins
        assert_eq!(
            rs.evaluate("edit", "x", Some(&vendor)),
            Some(PermissionAction::Deny)
        );
        // src => scoped Deny doesn't match, blanket Allow applies
        assert_eq!(
            rs.evaluate("edit", "x", Some(&src)),
            Some(PermissionAction::Allow)
        );
    }

    #[test]
    fn test_ruleset_prompt_action() {
        let mut rs = PermissionRuleSet::new();
        rs.add_rule(PermissionRule {
            pattern: "bash:sudo *".into(),
            action: PermissionAction::Prompt,
            priority: 50,
            directory_scope: None,
        });
        assert_eq!(
            rs.evaluate_simple("bash", "sudo rm -rf /"),
            Some(PermissionAction::Prompt)
        );
    }

    #[test]
    fn test_ruleset_remove_rules() {
        let mut rs = PermissionRuleSet::new();
        rs.add_rule(PermissionRule {
            pattern: "bash:*".into(),
            action: PermissionAction::Allow,
            priority: 1,
            directory_scope: None,
        });
        rs.add_rule(PermissionRule {
            pattern: "edit:*".into(),
            action: PermissionAction::Deny,
            priority: 1,
            directory_scope: None,
        });
        assert_eq!(rs.rules().len(), 2);
        rs.remove_rules(|r| r.action == PermissionAction::Deny);
        assert_eq!(rs.rules().len(), 1);
        assert_eq!(rs.rules()[0].pattern, "bash:*");
    }
}
