//! Approval rules system for pattern-based command approval.
//!
//! Rules can be session-only (ephemeral) or persistent across sessions.
//! Persistent rules are stored in:
//!   - User-global: `~/.opendev/permissions.json`
//!   - Project-scoped: `.opendev/permissions.json`
//!
//! Ported from `opendev/core/runtime/approval/rules.py`.

mod manager;
mod persistence;
mod types;

pub use manager::ApprovalRulesManager;
pub use types::{ApprovalRule, CommandHistory, RuleAction, RuleScope, RuleType};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
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
