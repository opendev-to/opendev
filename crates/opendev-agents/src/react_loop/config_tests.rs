use super::*;

#[test]
fn test_default_config() {
    let config = ReactLoopConfig::default();
    assert_eq!(config.max_iterations, Some(50));
    assert_eq!(config.max_nudge_attempts, 3);
    assert_eq!(config.max_todo_nudges, 4);
    assert!(config.permission.is_empty());
}

#[test]
fn test_evaluate_permission_empty_rules() {
    let config = ReactLoopConfig::default();
    assert!(config.evaluate_permission("read_file", "").is_none());
}

#[test]
fn test_evaluate_permission_with_action_rule() {
    let mut config = ReactLoopConfig::default();
    config.permission.insert(
        "run_command".to_string(),
        PermissionRule::Action(PermissionAction::Deny),
    );
    assert_eq!(
        config.evaluate_permission("run_command", ""),
        Some(PermissionAction::Deny)
    );
}

#[test]
fn test_evaluate_permission_no_match() {
    let mut config = ReactLoopConfig::default();
    config.permission.insert(
        "run_command".to_string(),
        PermissionRule::Action(PermissionAction::Deny),
    );
    assert!(config.evaluate_permission("read_file", "").is_none());
}
