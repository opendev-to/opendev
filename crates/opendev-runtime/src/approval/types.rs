//! Rule-related type definitions for the approval system.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tracing::warn;

/// Action to take when a rule matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    AutoApprove,
    AutoDeny,
    RequireApproval,
    RequireEdit,
}

/// How the rule pattern is matched against commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleType {
    /// Regex search within the command.
    Pattern,
    /// Exact string match.
    Command,
    /// Prefix match (exact or with trailing space + args).
    Prefix,
    /// Danger-pattern regex (same as Pattern but semantically distinct).
    Danger,
}

/// A single approval rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rule_type: RuleType,
    pub pattern: String,
    pub action: RuleAction,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,

    /// Compiled regex pattern, lazily initialized on first match.
    /// Skipped during serialization; rebuilt on demand via `OnceLock`.
    #[serde(skip)]
    pub(crate) compiled_regex: OnceLock<Option<Regex>>,
}

fn default_true() -> bool {
    true
}

impl ApprovalRule {
    /// Create a new approval rule.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        description: String,
        rule_type: RuleType,
        pattern: String,
        action: RuleAction,
        enabled: bool,
        priority: i32,
    ) -> Self {
        Self {
            id,
            name,
            description,
            rule_type,
            pattern,
            action,
            enabled,
            priority,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        }
    }

    /// Get the compiled regex, initializing it on first access.
    /// Returns `None` for non-regex rule types or invalid patterns.
    fn get_compiled_regex(&self) -> Option<&Regex> {
        if !matches!(self.rule_type, RuleType::Pattern | RuleType::Danger) {
            return None;
        }
        self.compiled_regex
            .get_or_init(|| match Regex::new(&self.pattern) {
                Ok(re) => Some(re),
                Err(e) => {
                    warn!("Invalid regex pattern '{}': {}", self.pattern, e);
                    None
                }
            })
            .as_ref()
    }

    /// Check whether this rule matches the given command string.
    pub fn matches(&self, command: &str) -> bool {
        if !self.enabled {
            return false;
        }
        match self.rule_type {
            RuleType::Pattern | RuleType::Danger => self
                .get_compiled_regex()
                .map(|re| re.is_match(command))
                .unwrap_or(false),
            RuleType::Command => command == self.pattern,
            RuleType::Prefix => {
                command == self.pattern || command.starts_with(&format!("{} ", self.pattern))
            }
        }
    }
}

/// Record of a command that was evaluated by the approval system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandHistory {
    pub command: String,
    pub approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_matched: Option<String>,
}

/// Persistence scope for rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleScope {
    User,
    Project,
    All,
}
