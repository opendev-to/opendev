//! ReactLoopConfig: configuration and permission evaluation.

use std::collections::HashMap;

use crate::subagents::spec::{PermissionAction, PermissionRule};

/// Configuration for the ReAct loop.
#[derive(Debug, Clone)]
pub struct ReactLoopConfig {
    /// Maximum number of iterations (None = unlimited).
    pub max_iterations: Option<usize>,
    /// Maximum consecutive no-tool-call responses before accepting completion.
    pub max_nudge_attempts: usize,
    /// Maximum todo completion nudges before allowing completion anyway.
    pub max_todo_nudges: usize,
    /// The user's original task text, used for completion nudge construction.
    pub original_task: Option<String>,
    /// Per-agent permission rules for tool access control.
    /// Maps tool name patterns to permission rules (allow/deny/ask).
    /// When non-empty, each tool call is checked against these rules
    /// before execution.
    pub permission: HashMap<String, PermissionRule>,
}

impl Default for ReactLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: Some(50), // 50 iterations is generous for real tasks; prevents headless hangs
            max_nudge_attempts: 3,
            max_todo_nudges: 4,
            original_task: None,
            permission: HashMap::new(),
        }
    }
}

impl ReactLoopConfig {
    /// Evaluate permission rules for a tool call.
    ///
    /// Returns `None` if no rules match (caller decides default behavior).
    /// `arg_pattern` is used for tools that have pattern-level rules (e.g. bash commands).
    pub fn evaluate_permission(
        &self,
        tool_name: &str,
        arg_pattern: &str,
    ) -> Option<PermissionAction> {
        use crate::subagents::spec::{glob_match, pattern_specificity};

        if self.permission.is_empty() {
            return None;
        }

        let mut best_match: Option<(PermissionAction, usize)> = None;

        for (tool_pattern, rule) in &self.permission {
            if !glob_match(tool_pattern, tool_name) {
                continue;
            }
            match rule {
                PermissionRule::Action(action) => {
                    let specificity = pattern_specificity(tool_pattern);
                    if best_match.as_ref().is_none_or(|(_, s)| specificity >= *s) {
                        best_match = Some((*action, specificity));
                    }
                }
                PermissionRule::Patterns(patterns) => {
                    for (pattern, action) in patterns {
                        if glob_match(pattern, arg_pattern) {
                            let specificity = pattern_specificity(pattern);
                            if best_match.as_ref().is_none_or(|(_, s)| specificity >= *s) {
                                best_match = Some((*action, specificity));
                            }
                        }
                    }
                }
            }
        }

        best_match.map(|(action, _)| action)
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
