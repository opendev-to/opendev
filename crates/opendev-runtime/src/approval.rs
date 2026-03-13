//! Approval rules system for pattern-based command approval.
//!
//! Rules can be session-only (ephemeral) or persistent across sessions.
//! Persistent rules are stored in:
//!   - User-global: `~/.opendev/permissions.json`
//!   - Project-scoped: `.opendev/permissions.json`
//!
//! Ported from `opendev/core/runtime/approval/rules.py`.

use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing::{debug, warn};

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
    compiled_regex: OnceLock<Option<Regex>>,
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

/// On-disk format for permissions.json.
#[derive(Debug, Serialize, Deserialize)]
struct PermissionsFile {
    version: u32,
    rules: Vec<ApprovalRule>,
}

/// Manager for approval rules and command history.
///
/// Supports both session-only (ephemeral) and persistent rules.
/// Persistent rules are loaded from disk on init and survive across sessions.
pub struct ApprovalRulesManager {
    rules: Vec<ApprovalRule>,
    history: Vec<CommandHistory>,
    project_dir: Option<PathBuf>,
}

impl ApprovalRulesManager {
    /// User-global permissions path: `~/.opendev/permissions.json`.
    fn user_permissions_path() -> Option<PathBuf> {
        dirs_next::home_dir().map(|h| h.join(".opendev").join("permissions.json"))
    }

    /// Create a new manager, loading default danger rules and persistent rules.
    pub fn new(project_dir: Option<&Path>) -> Self {
        let mut mgr = Self {
            rules: Vec::new(),
            history: Vec::new(),
            project_dir: project_dir.map(|p| p.to_path_buf()),
        };
        mgr.initialize_default_rules();
        mgr.load_persistent_rules();
        mgr
    }

    /// Read-only access to the current rule set.
    pub fn rules(&self) -> &[ApprovalRule] {
        &self.rules
    }

    /// Read-only access to command history.
    pub fn history(&self) -> &[CommandHistory] {
        &self.history
    }

    // ------------------------------------------------------------------
    // Default rules
    // ------------------------------------------------------------------

    fn initialize_default_rules(&mut self) {
        let now = Utc::now().to_rfc3339();
        self.rules.push(ApprovalRule {
            id: "default_danger_rm".to_string(),
            name: "Dangerous rm commands".to_string(),
            description: "Require approval for dangerous rm commands".to_string(),
            rule_type: RuleType::Danger,
            pattern: r"rm\s+(-rf?|-fr?)\s+(/|\*|~)".to_string(),
            action: RuleAction::RequireApproval,
            enabled: true,
            priority: 100,
            created_at: Some(now.clone()),
            modified_at: None,
            compiled_regex: OnceLock::new(),
        });
        self.rules.push(ApprovalRule {
            id: "default_danger_chmod".to_string(),
            name: "Dangerous chmod 777".to_string(),
            description: "Require approval for chmod 777".to_string(),
            rule_type: RuleType::Danger,
            pattern: r"chmod\s+777".to_string(),
            action: RuleAction::RequireApproval,
            enabled: true,
            priority: 100,
            created_at: Some(now),
            modified_at: None,
            compiled_regex: OnceLock::new(),
        });
    }

    // ------------------------------------------------------------------
    // Rule evaluation
    // ------------------------------------------------------------------

    /// Evaluate a command against all enabled rules (highest priority first).
    ///
    /// Returns the first matching rule, or `None` if no rule applies.
    pub fn evaluate_command(&self, command: &str) -> Option<&ApprovalRule> {
        let mut enabled: Vec<&ApprovalRule> = self.rules.iter().filter(|r| r.enabled).collect();
        enabled.sort_by(|a, b| b.priority.cmp(&a.priority));
        enabled.into_iter().find(|r| r.matches(command))
    }

    // ------------------------------------------------------------------
    // CRUD
    // ------------------------------------------------------------------

    /// Add a session-only rule.
    pub fn add_rule(&mut self, rule: ApprovalRule) {
        self.rules.push(rule);
    }

    /// Update fields on an existing rule by ID.
    ///
    /// Returns `true` if a rule was found and updated.
    pub fn update_rule<F>(&mut self, rule_id: &str, updater: F) -> bool
    where
        F: FnOnce(&mut ApprovalRule),
    {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == rule_id) {
            updater(rule);
            rule.modified_at = Some(Utc::now().to_rfc3339());
            true
        } else {
            false
        }
    }

    /// Remove a rule by ID. Returns `true` if something was removed.
    pub fn remove_rule(&mut self, rule_id: &str) -> bool {
        let before = self.rules.len();
        self.rules.retain(|r| r.id != rule_id);
        self.rules.len() != before
    }

    // ------------------------------------------------------------------
    // History
    // ------------------------------------------------------------------

    /// Record a command evaluation in the session history.
    pub fn add_history(
        &mut self,
        command: &str,
        approved: bool,
        edited_command: Option<String>,
        rule_matched: Option<String>,
    ) {
        self.history.push(CommandHistory {
            command: command.to_string(),
            approved,
            edited_command,
            timestamp: Some(Utc::now().to_rfc3339()),
            rule_matched,
        });
    }

    // ------------------------------------------------------------------
    // Persistent rules
    // ------------------------------------------------------------------

    /// Add a rule and persist it to disk.
    pub fn add_persistent_rule(&mut self, rule: ApprovalRule, scope: RuleScope) {
        self.add_rule(rule);
        self.save_persistent_rules(scope);
    }

    /// Remove a rule and update persistent storage.
    pub fn remove_persistent_rule(&mut self, rule_id: &str) -> bool {
        let removed = self.remove_rule(rule_id);
        if removed {
            self.save_persistent_rules(RuleScope::User);
            if self.project_dir.is_some() {
                self.save_persistent_rules(RuleScope::Project);
            }
        }
        removed
    }

    /// Remove all persistent (non-default) rules. Returns count removed.
    pub fn clear_persistent_rules(&mut self, scope: RuleScope) -> usize {
        let before = self.rules.len();
        self.rules.retain(|r| r.id.starts_with("default_"));
        let removed = before - self.rules.len();

        if matches!(scope, RuleScope::User | RuleScope::All)
            && let Some(path) = Self::user_permissions_path()
        {
            Self::delete_permissions_file(&path);
        }
        if matches!(scope, RuleScope::Project | RuleScope::All)
            && let Some(ref dir) = self.project_dir
        {
            Self::delete_permissions_file(&dir.join(".opendev").join("permissions.json"));
        }

        removed
    }

    /// List all non-default rules in a display-friendly format.
    pub fn list_persistent_rules(&self) -> Vec<serde_json::Value> {
        self.rules
            .iter()
            .filter(|r| !r.id.starts_with("default_"))
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "name": r.name,
                    "pattern": r.pattern,
                    "action": r.action,
                    "type": r.rule_type,
                    "enabled": r.enabled,
                })
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Persistence internals
    // ------------------------------------------------------------------

    fn load_persistent_rules(&mut self) {
        // User-global rules
        if let Some(path) = Self::user_permissions_path() {
            self.load_rules_from_file(&path);
        }
        // Project-scoped rules (higher priority, loaded second)
        if let Some(ref dir) = self.project_dir {
            self.load_rules_from_file(&dir.join(".opendev").join("permissions.json"));
        }
    }

    fn load_rules_from_file(&mut self, path: &Path) {
        if !path.exists() {
            return;
        }
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<PermissionsFile>(&content) {
                Ok(data) => {
                    let count = data.rules.len();
                    for rule in data.rules {
                        // Skip duplicates
                        if self.rules.iter().any(|r| r.id == rule.id) {
                            continue;
                        }
                        self.rules.push(rule);
                    }
                    debug!("Loaded {} rules from {}", count, path.display());
                }
                Err(e) => {
                    warn!(
                        "Failed to parse persistent rules from {}: {}",
                        path.display(),
                        e
                    );
                }
            },
            Err(e) => {
                warn!(
                    "Failed to read persistent rules from {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }

    fn save_persistent_rules(&self, scope: RuleScope) {
        let persistent: Vec<&ApprovalRule> = self
            .rules
            .iter()
            .filter(|r| !r.id.starts_with("default_"))
            .collect();
        let data = PermissionsFile {
            version: 1,
            rules: persistent.into_iter().cloned().collect(),
        };

        let path = match scope {
            RuleScope::User => Self::user_permissions_path(),
            RuleScope::Project => self
                .project_dir
                .as_ref()
                .map(|d| d.join(".opendev").join("permissions.json")),
            RuleScope::All => {
                // Save to both
                self.save_persistent_rules(RuleScope::User);
                if self.project_dir.is_some() {
                    self.save_persistent_rules(RuleScope::Project);
                }
                return;
            }
        };

        let Some(path) = path else { return };

        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            warn!("Failed to create directory {}: {}", parent.display(), e);
            return;
        }

        match serde_json::to_string_pretty(&data) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    warn!(
                        "Failed to save persistent rules to {}: {}",
                        path.display(),
                        e
                    );
                } else {
                    debug!("Saved {} rules to {}", data.rules.len(), path.display());
                }
            }
            Err(e) => {
                warn!("Failed to serialize rules: {}", e);
            }
        }
    }

    fn delete_permissions_file(path: &Path) {
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_rule_pattern_match() {
        let rule = ApprovalRule {
            id: "test".into(),
            name: "test".into(),
            description: "test".into(),
            rule_type: RuleType::Pattern,
            pattern: r"rm\s+-rf".into(),
            action: RuleAction::RequireApproval,
            enabled: true,
            priority: 0,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        };
        assert!(rule.matches("rm -rf /tmp"));
        assert!(!rule.matches("ls -la"));
        // Verify regex was cached after first match
        assert!(rule.compiled_regex.get().is_some());
    }

    #[test]
    fn test_rule_command_match() {
        let rule = ApprovalRule {
            id: "test".into(),
            name: "test".into(),
            description: "test".into(),
            rule_type: RuleType::Command,
            pattern: "deploy".into(),
            action: RuleAction::AutoApprove,
            enabled: true,
            priority: 0,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        };
        assert!(rule.matches("deploy"));
        assert!(!rule.matches("deploy --force"));
    }

    #[test]
    fn test_rule_prefix_match() {
        let rule = ApprovalRule {
            id: "test".into(),
            name: "test".into(),
            description: "test".into(),
            rule_type: RuleType::Prefix,
            pattern: "git push".into(),
            action: RuleAction::RequireApproval,
            enabled: true,
            priority: 0,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        };
        assert!(rule.matches("git push"));
        assert!(rule.matches("git push --force"));
        assert!(!rule.matches("git pull"));
        // Must not match prefix without space boundary
        assert!(!rule.matches("git pushx"));
    }

    #[test]
    fn test_disabled_rule_never_matches() {
        let rule = ApprovalRule {
            id: "test".into(),
            name: "test".into(),
            description: "test".into(),
            rule_type: RuleType::Pattern,
            pattern: r".*".into(),
            action: RuleAction::AutoApprove,
            enabled: false,
            priority: 0,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        };
        assert!(!rule.matches("anything"));
    }

    #[test]
    fn test_invalid_regex_returns_false() {
        let rule = ApprovalRule {
            id: "test".into(),
            name: "test".into(),
            description: "test".into(),
            rule_type: RuleType::Pattern,
            pattern: r"[invalid".into(),
            action: RuleAction::AutoApprove,
            enabled: true,
            priority: 0,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        };
        assert!(!rule.matches("anything"));
    }

    #[test]
    fn test_manager_default_rules() {
        let mgr = ApprovalRulesManager::new(None);
        assert!(mgr.rules().len() >= 2);
        assert!(mgr.rules().iter().any(|r| r.id == "default_danger_rm"));
        assert!(mgr.rules().iter().any(|r| r.id == "default_danger_chmod"));
    }

    #[test]
    fn test_evaluate_command_priority() {
        let mut mgr = ApprovalRulesManager::new(None);
        mgr.add_rule(ApprovalRule {
            id: "low".into(),
            name: "low".into(),
            description: "low priority".into(),
            rule_type: RuleType::Pattern,
            pattern: r"rm".into(),
            action: RuleAction::AutoApprove,
            enabled: true,
            priority: 1,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        });
        mgr.add_rule(ApprovalRule {
            id: "high".into(),
            name: "high".into(),
            description: "high priority".into(),
            rule_type: RuleType::Pattern,
            pattern: r"rm".into(),
            action: RuleAction::AutoDeny,
            enabled: true,
            priority: 50,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        });
        // The default danger rule has priority 100, should win
        let matched = mgr.evaluate_command("rm -rf /");
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().id, "default_danger_rm");
    }

    #[test]
    fn test_add_remove_rule() {
        let mut mgr = ApprovalRulesManager::new(None);
        let initial = mgr.rules().len();
        mgr.add_rule(ApprovalRule {
            id: "custom".into(),
            name: "custom".into(),
            description: "custom".into(),
            rule_type: RuleType::Command,
            pattern: "test".into(),
            action: RuleAction::AutoApprove,
            enabled: true,
            priority: 0,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        });
        assert_eq!(mgr.rules().len(), initial + 1);
        assert!(mgr.remove_rule("custom"));
        assert_eq!(mgr.rules().len(), initial);
        assert!(!mgr.remove_rule("nonexistent"));
    }

    #[test]
    fn test_update_rule() {
        let mut mgr = ApprovalRulesManager::new(None);
        mgr.add_rule(ApprovalRule {
            id: "updatable".into(),
            name: "old name".into(),
            description: "desc".into(),
            rule_type: RuleType::Command,
            pattern: "test".into(),
            action: RuleAction::AutoApprove,
            enabled: true,
            priority: 0,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        });
        assert!(mgr.update_rule("updatable", |r| {
            r.name = "new name".into();
            r.enabled = false;
        }));
        let rule = mgr.rules().iter().find(|r| r.id == "updatable").unwrap();
        assert_eq!(rule.name, "new name");
        assert!(!rule.enabled);
        assert!(rule.modified_at.is_some());

        assert!(!mgr.update_rule("nonexistent", |_| {}));
    }

    #[test]
    fn test_history() {
        let mut mgr = ApprovalRulesManager::new(None);
        assert!(mgr.history().is_empty());
        mgr.add_history("ls -la", true, None, None);
        mgr.add_history("rm -rf /", false, None, Some("default_danger_rm".into()));
        assert_eq!(mgr.history().len(), 2);
        assert!(mgr.history()[0].approved);
        assert!(!mgr.history()[1].approved);
    }

    #[test]
    fn test_persistent_rules_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let project_dir = tmp.path();

        // Create a manager with a persistent rule
        {
            let mut mgr = ApprovalRulesManager::new(Some(project_dir));
            mgr.add_persistent_rule(
                ApprovalRule {
                    id: "persist_test".into(),
                    name: "persist".into(),
                    description: "test".into(),
                    rule_type: RuleType::Command,
                    pattern: "deploy".into(),
                    action: RuleAction::RequireApproval,
                    enabled: true,
                    priority: 10,
                    created_at: None,
                    modified_at: None,
                    compiled_regex: OnceLock::new(),
                },
                RuleScope::Project,
            );
        }

        // New manager should load it back
        let mgr2 = ApprovalRulesManager::new(Some(project_dir));
        assert!(mgr2.rules().iter().any(|r| r.id == "persist_test"));
    }

    #[test]
    fn test_clear_persistent_rules() {
        let tmp = TempDir::new().unwrap();
        let project_dir = tmp.path();

        let mut mgr = ApprovalRulesManager::new(Some(project_dir));
        mgr.add_persistent_rule(
            ApprovalRule {
                id: "will_be_cleared".into(),
                name: "temp".into(),
                description: "temp".into(),
                rule_type: RuleType::Command,
                pattern: "test".into(),
                action: RuleAction::AutoApprove,
                enabled: true,
                priority: 0,
                created_at: None,
                modified_at: None,
                compiled_regex: OnceLock::new(),
            },
            RuleScope::Project,
        );
        let removed = mgr.clear_persistent_rules(RuleScope::All);
        assert_eq!(removed, 1);
        // Only defaults remain
        assert!(mgr.rules().iter().all(|r| r.id.starts_with("default_")));
    }

    #[test]
    fn test_list_persistent_rules_excludes_defaults() {
        let mut mgr = ApprovalRulesManager::new(None);
        mgr.add_rule(ApprovalRule {
            id: "custom_rule".into(),
            name: "custom".into(),
            description: "custom".into(),
            rule_type: RuleType::Command,
            pattern: "test".into(),
            action: RuleAction::AutoApprove,
            enabled: true,
            priority: 0,
            created_at: None,
            modified_at: None,
            compiled_regex: OnceLock::new(),
        });
        let listing = mgr.list_persistent_rules();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0]["id"], "custom_rule");
    }
}
