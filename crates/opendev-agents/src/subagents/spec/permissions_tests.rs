use super::super::types::SubAgentSpec;
use super::*;

#[test]
fn test_glob_match_basic() {
    assert!(glob_match("*", "anything"));
    assert!(glob_match("read_*", "read_file"));
    assert!(glob_match("read_*", "read_dir"));
    assert!(!glob_match("read_*", "write_file"));
    assert!(glob_match("?at", "cat"));
    assert!(!glob_match("?at", "chat"));
    assert!(glob_match("git *", "git status"));
    assert!(glob_match("git *", "git push origin main"));
}

#[test]
fn test_glob_match_exact() {
    assert!(glob_match("bash", "bash"));
    assert!(!glob_match("bash", "bash2"));
    assert!(!glob_match("bash2", "bash"));
}

#[test]
fn test_permission_action_serde() {
    let action = PermissionAction::Allow;
    let json = serde_json::to_string(&action).unwrap();
    assert_eq!(json, "\"allow\"");
    let restored: PermissionAction = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, PermissionAction::Allow);
}

#[test]
fn test_permission_rule_single_action() {
    let rule: PermissionRule = serde_json::from_str("\"deny\"").unwrap();
    assert!(matches!(
        rule,
        PermissionRule::Action(PermissionAction::Deny)
    ));
}

#[test]
fn test_permission_rule_patterns() {
    let json = r#"{"*": "ask", "git *": "allow", "rm -rf *": "deny"}"#;
    let rule: PermissionRule = serde_json::from_str(json).unwrap();
    if let PermissionRule::Patterns(p) = &rule {
        assert_eq!(p.len(), 3);
        assert_eq!(p["*"], PermissionAction::Ask);
        assert_eq!(p["git *"], PermissionAction::Allow);
        assert_eq!(p["rm -rf *"], PermissionAction::Deny);
    } else {
        panic!("Expected Patterns variant");
    }
}

#[test]
fn test_permission_serde_roundtrip() {
    let mut patterns = HashMap::new();
    patterns.insert("*".to_string(), PermissionAction::Ask);
    patterns.insert("git *".to_string(), PermissionAction::Allow);

    let mut perms = HashMap::new();
    perms.insert("bash".to_string(), PermissionRule::Patterns(patterns));
    perms.insert(
        "edit".to_string(),
        PermissionRule::Action(PermissionAction::Deny),
    );

    let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

    let json = serde_json::to_string(&spec).unwrap();
    let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();

    assert_eq!(
        restored.evaluate_permission("bash", "git status"),
        Some(PermissionAction::Allow)
    );
    assert_eq!(
        restored.evaluate_permission("edit", "any_file"),
        Some(PermissionAction::Deny)
    );
}

#[test]
fn test_permission_skipped_when_empty() {
    let spec = SubAgentSpec::new("test", "desc", "prompt");
    let json = serde_json::to_string(&spec).unwrap();
    // The "permission" map field (not "permission_mode") should be omitted when empty
    assert!(
        !json.contains("\"permission\":{") && !json.contains("\"permission\":{}"),
        "Empty permission map should be skipped in serialization, got: {json}"
    );
}
