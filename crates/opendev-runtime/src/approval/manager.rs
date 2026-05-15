//! `ApprovalRulesManager` — manages approval rules and command history.

use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::persistence;
use super::types::{ApprovalRule, CommandHistory, RuleAction, RuleScope, RuleType};

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
    /// Create a new manager, loading default danger rules and persistent rules.
    pub fn new(project_dir: Option<&Path>) -> Self {
        let mut mgr = Self {
            rules: Vec::new(),
            history: Vec::new(),
            project_dir: project_dir.map(|p| p.to_path_buf()),
        };
        mgr.initialize_default_rules();
        persistence::load_persistent_rules(&mut mgr.rules, project_dir);
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
            created_at: Some(now.clone()),
            modified_at: None,
            compiled_regex: OnceLock::new(),
        });
        self.rules.push(ApprovalRule {
            id: "default_danger_git_force_push".to_string(),
            name: "Git force push to protected branches".to_string(),
            description:
                "Require approval for force push to main/master/develop/production/staging"
                    .to_string(),
            rule_type: RuleType::Danger,
            pattern: r"git\s+push\s+.*--force.*\b(main|master|develop|production|staging)\b"
                .to_string(),
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
        enabled.sort_by_key(|r| std::cmp::Reverse(r.priority));
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
        persistence::save_persistent_rules(&self.rules, self.project_dir.as_deref(), scope);
    }

    /// Remove a rule and update persistent storage.
    pub fn remove_persistent_rule(&mut self, rule_id: &str) -> bool {
        let removed = self.remove_rule(rule_id);
        if removed {
            persistence::save_persistent_rules(
                &self.rules,
                self.project_dir.as_deref(),
                RuleScope::User,
            );
            if self.project_dir.is_some() {
                persistence::save_persistent_rules(
                    &self.rules,
                    self.project_dir.as_deref(),
                    RuleScope::Project,
                );
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
            && let Some(path) = persistence::user_permissions_path()
        {
            persistence::delete_permissions_file(&path);
        }
        if matches!(scope, RuleScope::Project | RuleScope::All)
            && let Some(ref dir) = self.project_dir
        {
            persistence::delete_permissions_file(&dir.join(".opendev").join("permissions.json"));
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
}
