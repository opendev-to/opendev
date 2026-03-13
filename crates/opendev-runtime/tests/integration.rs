//! Integration tests for the runtime crate.
//!
//! Tests approval rules (pattern/command/prefix/danger matching, priority
//! evaluation, persistent rules roundtrip), cost tracking, interrupt tokens,
//! and todo management.

use opendev_runtime::{
    ApprovalRule, ApprovalRulesManager, CostTracker, InterruptToken, RuleAction, RuleScope,
    RuleType, TodoManager, TodoStatus,
};
use tempfile::TempDir;

fn make_rule(
    id: &str,
    rule_type: RuleType,
    pattern: &str,
    action: RuleAction,
    priority: i32,
) -> ApprovalRule {
    ApprovalRule::new(
        id.to_string(),
        id.to_string(),
        format!("Test rule: {id}"),
        rule_type,
        pattern.to_string(),
        action,
        true,
        priority,
    )
}

// ========================================================================
// Rule matching for all RuleTypes
// ========================================================================

/// Pattern rules use regex matching.
#[test]
fn pattern_rule_regex_matching() {
    let rule = make_rule(
        "p1",
        RuleType::Pattern,
        r"rm\s+-rf",
        RuleAction::AutoDeny,
        0,
    );
    assert!(rule.matches("rm -rf /tmp/stuff"));
    assert!(rule.matches("sudo rm -rf /"));
    assert!(!rule.matches("ls -la"));
    assert!(!rule.matches("remove-file"));
}

/// Command rules use exact string matching.
#[test]
fn command_rule_exact_matching() {
    let rule = make_rule(
        "c1",
        RuleType::Command,
        "deploy",
        RuleAction::RequireApproval,
        0,
    );
    assert!(rule.matches("deploy"));
    assert!(!rule.matches("deploy --force"));
    assert!(!rule.matches("Deploy"));
    assert!(!rule.matches("redeploy"));
}

/// Prefix rules match exact command or command + space + args.
#[test]
fn prefix_rule_boundary_matching() {
    let rule = make_rule(
        "px1",
        RuleType::Prefix,
        "git push",
        RuleAction::RequireApproval,
        0,
    );
    assert!(rule.matches("git push"));
    assert!(rule.matches("git push --force origin main"));
    assert!(!rule.matches("git pull"));
    assert!(!rule.matches("git pushx")); // must have space boundary
    assert!(!rule.matches("git pusher"));
}

/// Danger rules behave like Pattern rules (regex).
#[test]
fn danger_rule_regex_matching() {
    let rule = make_rule(
        "d1",
        RuleType::Danger,
        r"chmod\s+777",
        RuleAction::RequireApproval,
        0,
    );
    assert!(rule.matches("chmod 777 /etc/passwd"));
    assert!(rule.matches("sudo chmod 777 ."));
    assert!(!rule.matches("chmod 755 file"));
}

/// Disabled rules never match regardless of pattern.
#[test]
fn disabled_rule_never_matches() {
    let mut rule = make_rule("dis", RuleType::Pattern, r".*", RuleAction::AutoApprove, 0);
    rule.enabled = false;
    assert!(!rule.matches("anything at all"));
}

/// Invalid regex in a pattern rule returns false (no panic).
#[test]
fn invalid_regex_returns_false() {
    let rule = make_rule(
        "bad",
        RuleType::Pattern,
        r"[unclosed",
        RuleAction::AutoDeny,
        0,
    );
    assert!(!rule.matches("anything"));
}

// ========================================================================
// Priority-based evaluation
// ========================================================================

/// evaluate_command returns the highest-priority matching rule.
#[test]
fn evaluate_command_highest_priority_wins() {
    let mut mgr = ApprovalRulesManager::new(None);

    // Low priority: auto-approve anything with "rm"
    mgr.add_rule(make_rule(
        "low",
        RuleType::Pattern,
        r"rm",
        RuleAction::AutoApprove,
        10,
    ));

    // High priority: deny dangerous rm
    mgr.add_rule(make_rule(
        "high",
        RuleType::Pattern,
        r"rm\s+-rf",
        RuleAction::AutoDeny,
        50,
    ));

    // Default danger_rm rule has priority 100, but let's test our custom rules
    // "rm -rf /" matches both our rules + default. Default has priority 100.
    let matched = mgr.evaluate_command("rm -rf /");
    assert!(matched.is_some());
    // default_danger_rm at priority 100 should win
    assert_eq!(matched.unwrap().id, "default_danger_rm");

    // "rm file.txt" only matches "low" and default
    // default_danger_rm pattern is `rm\s+(-rf?|-fr?)\s+(/|\*|~)`, which doesn't match "rm file.txt"
    let matched = mgr.evaluate_command("rm file.txt");
    assert!(matched.is_some());
    assert_eq!(matched.unwrap().id, "low");
    assert_eq!(matched.unwrap().action, RuleAction::AutoApprove);
}

/// When no rules match, evaluate_command returns None.
#[test]
fn evaluate_command_no_match_returns_none() {
    let mgr = ApprovalRulesManager::new(None);
    // "ls -la" doesn't match any default danger rules
    let result = mgr.evaluate_command("ls -la");
    assert!(result.is_none());
}

// ========================================================================
// Default rules
// ========================================================================

/// Manager initializes with at least 2 default danger rules.
#[test]
fn default_rules_present() {
    let mgr = ApprovalRulesManager::new(None);
    let rules = mgr.rules();
    assert!(rules.len() >= 2);

    let rm_rule = rules.iter().find(|r| r.id == "default_danger_rm").unwrap();
    assert_eq!(rm_rule.rule_type, RuleType::Danger);
    assert_eq!(rm_rule.action, RuleAction::RequireApproval);
    assert_eq!(rm_rule.priority, 100);
    assert!(rm_rule.enabled);

    let chmod_rule = rules
        .iter()
        .find(|r| r.id == "default_danger_chmod")
        .unwrap();
    assert!(chmod_rule.matches("chmod 777 /etc"));
}

// ========================================================================
// CRUD operations
// ========================================================================

/// Add, update, and remove rules.
#[test]
fn rule_crud_operations() {
    let mut mgr = ApprovalRulesManager::new(None);
    let initial_count = mgr.rules().len();

    // Add
    mgr.add_rule(make_rule(
        "crud-test",
        RuleType::Command,
        "test",
        RuleAction::AutoApprove,
        0,
    ));
    assert_eq!(mgr.rules().len(), initial_count + 1);

    // Update
    let updated = mgr.update_rule("crud-test", |r| {
        r.name = "Updated Name".to_string();
        r.action = RuleAction::AutoDeny;
    });
    assert!(updated);
    let rule = mgr.rules().iter().find(|r| r.id == "crud-test").unwrap();
    assert_eq!(rule.name, "Updated Name");
    assert_eq!(rule.action, RuleAction::AutoDeny);
    assert!(rule.modified_at.is_some());

    // Update nonexistent returns false
    assert!(!mgr.update_rule("nonexistent", |_| {}));

    // Remove
    assert!(mgr.remove_rule("crud-test"));
    assert_eq!(mgr.rules().len(), initial_count);
    assert!(!mgr.remove_rule("crud-test")); // already gone
}

// ========================================================================
// Persistent rules roundtrip
// ========================================================================

/// Rules persisted to project scope survive manager recreation.
#[test]
fn persistent_rules_project_scope_roundtrip() {
    let tmp = TempDir::new().unwrap();

    // Create and persist a rule
    {
        let mut mgr = ApprovalRulesManager::new(Some(tmp.path()));
        mgr.add_persistent_rule(
            make_rule(
                "persist-1",
                RuleType::Command,
                "deploy",
                RuleAction::RequireApproval,
                25,
            ),
            RuleScope::Project,
        );
        mgr.add_persistent_rule(
            make_rule(
                "persist-2",
                RuleType::Prefix,
                "docker push",
                RuleAction::AutoDeny,
                30,
            ),
            RuleScope::Project,
        );
    }

    // New manager should load persisted rules
    let mgr2 = ApprovalRulesManager::new(Some(tmp.path()));
    assert!(mgr2.rules().iter().any(|r| r.id == "persist-1"));
    assert!(mgr2.rules().iter().any(|r| r.id == "persist-2"));

    let p1 = mgr2.rules().iter().find(|r| r.id == "persist-1").unwrap();
    assert_eq!(p1.pattern, "deploy");
    assert_eq!(p1.action, RuleAction::RequireApproval);
    assert_eq!(p1.priority, 25);

    let p2 = mgr2.rules().iter().find(|r| r.id == "persist-2").unwrap();
    assert_eq!(p2.rule_type, RuleType::Prefix);
    assert_eq!(p2.pattern, "docker push");
}

/// Clear persistent rules removes non-default rules.
#[test]
fn clear_persistent_rules_keeps_defaults() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = ApprovalRulesManager::new(Some(tmp.path()));

    mgr.add_persistent_rule(
        make_rule("temp", RuleType::Command, "x", RuleAction::AutoApprove, 0),
        RuleScope::Project,
    );

    let removed = mgr.clear_persistent_rules(RuleScope::All);
    assert_eq!(removed, 1);

    // Only defaults remain
    for rule in mgr.rules() {
        assert!(
            rule.id.starts_with("default_"),
            "non-default rule survived: {}",
            rule.id
        );
    }
}

/// list_persistent_rules excludes default rules.
#[test]
fn list_persistent_excludes_defaults() {
    let mut mgr = ApprovalRulesManager::new(None);
    mgr.add_rule(make_rule(
        "custom",
        RuleType::Command,
        "x",
        RuleAction::AutoApprove,
        0,
    ));

    let listing = mgr.list_persistent_rules();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0]["id"], "custom");
    // No default rules in listing
    assert!(!listing.iter().any(|r| {
        r["id"]
            .as_str()
            .map(|s| s.starts_with("default_"))
            .unwrap_or(false)
    }));
}

// ========================================================================
// History tracking
// ========================================================================

/// Command history records approved and denied commands.
#[test]
fn command_history_records() {
    let mut mgr = ApprovalRulesManager::new(None);
    assert!(mgr.history().is_empty());

    mgr.add_history("ls -la", true, None, None);
    mgr.add_history("rm -rf /", false, None, Some("default_danger_rm".into()));
    mgr.add_history(
        "mv bad.sh",
        true,
        Some("mv safe.sh".into()),
        Some("custom_rule".into()),
    );

    assert_eq!(mgr.history().len(), 3);

    assert!(mgr.history()[0].approved);
    assert_eq!(mgr.history()[0].command, "ls -la");
    assert!(mgr.history()[0].timestamp.is_some());

    assert!(!mgr.history()[1].approved);
    assert_eq!(
        mgr.history()[1].rule_matched.as_deref(),
        Some("default_danger_rm")
    );

    assert_eq!(
        mgr.history()[2].edited_command.as_deref(),
        Some("mv safe.sh")
    );
}

// ========================================================================
// Cost tracker
// ========================================================================

/// CostTracker accumulates token usage and costs across multiple calls.
#[test]
fn cost_tracker_accumulates() {
    use opendev_runtime::TokenUsage;

    let mut tracker = CostTracker::new();

    let usage1 = TokenUsage {
        prompt_tokens: 100,
        completion_tokens: 50,
        ..Default::default()
    };
    let usage2 = TokenUsage {
        prompt_tokens: 200,
        completion_tokens: 100,
        ..Default::default()
    };

    tracker.record_usage(&usage1, None);
    tracker.record_usage(&usage2, None);

    assert_eq!(tracker.total_input_tokens, 300);
    assert_eq!(tracker.total_output_tokens, 150);
    assert_eq!(tracker.call_count, 2);
}

/// CostTracker computes cost when pricing info is provided.
#[test]
fn cost_tracker_with_pricing() {
    use opendev_runtime::{PricingInfo, TokenUsage};

    let mut tracker = CostTracker::new();
    let pricing = PricingInfo {
        input_price_per_million: 3.0,   // $3/M input tokens
        output_price_per_million: 15.0, // $15/M output tokens
    };

    let usage = TokenUsage {
        prompt_tokens: 1_000_000,
        completion_tokens: 100_000,
        ..Default::default()
    };

    let cost = tracker.record_usage(&usage, Some(&pricing));
    assert!(cost > 0.0, "cost should be positive with pricing");
    assert!(tracker.total_cost_usd > 0.0);
}

// ========================================================================
// Interrupt token
// ========================================================================

/// InterruptToken starts un-requested and can be requested.
#[test]
fn interrupt_token_lifecycle() {
    let token = InterruptToken::new();
    assert!(!token.is_requested());

    token.request();
    assert!(token.is_requested());
}

/// Clone of InterruptToken shares cancellation state.
#[test]
fn interrupt_token_clone_shares_state() {
    let token = InterruptToken::new();
    let clone = token.clone();

    assert!(!clone.is_requested());
    token.request();
    assert!(clone.is_requested());
}

/// throw_if_requested returns Err after request.
#[test]
fn interrupt_token_throw_if_requested() {
    let token = InterruptToken::new();
    assert!(token.throw_if_requested().is_ok());

    token.request();
    assert!(token.throw_if_requested().is_err());
}

/// Reset clears the interrupt state.
#[test]
fn interrupt_token_reset() {
    let token = InterruptToken::new();
    token.request();
    assert!(token.is_requested());

    token.reset();
    assert!(!token.is_requested());
}

// ========================================================================
// Todo management
// ========================================================================

/// TodoManager tracks items with status transitions.
#[test]
fn todo_manager_crud_and_status() {
    let mut mgr = TodoManager::new();

    let id1 = mgr.add("Write tests".to_string());
    let id2 = mgr.add("Review PR".to_string());
    let id3 = mgr.add("Deploy".to_string());

    assert_eq!(mgr.total(), 3);
    assert!(mgr.has_todos());

    // All start as pending
    assert_eq!(mgr.pending_count(), 3);
    assert_eq!(mgr.completed_count(), 0);

    // Start an item (pending -> in_progress)
    mgr.start(id1);
    assert_eq!(mgr.in_progress_count(), 1);

    // Complete an item (in_progress -> completed)
    mgr.complete(id1);
    assert_eq!(mgr.completed_count(), 1);
    assert_eq!(mgr.pending_count(), 2);

    // Set status directly
    mgr.set_status(id2, TodoStatus::Completed);
    assert_eq!(mgr.completed_count(), 2);

    // Not all completed yet
    assert!(!mgr.all_completed());

    // Complete the last one
    mgr.complete(id3);
    assert!(mgr.all_completed());
}

/// TodoManager.next_pending returns the first pending item.
#[test]
fn todo_next_pending() {
    let mut mgr = TodoManager::new();
    let id1 = mgr.add("First".to_string());
    let _id2 = mgr.add("Second".to_string());

    let next = mgr.next_pending().unwrap();
    assert_eq!(next.title, "First");

    mgr.complete(id1);
    let next = mgr.next_pending().unwrap();
    assert_eq!(next.title, "Second");
}

/// parse_plan_steps extracts numbered/bulleted steps from plan text.
#[test]
fn parse_plan_steps_from_markdown() {
    use opendev_runtime::parse_plan_steps;

    let plan = "# Implementation Plan\n\n1. Read the existing code\n2. Write new tests\n3. Refactor the module\n";
    let steps = parse_plan_steps(plan);
    assert!(steps.len() >= 3);
}
