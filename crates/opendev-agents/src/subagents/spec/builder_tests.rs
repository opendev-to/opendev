use super::*;

#[test]
fn test_subagent_spec_with_tools() {
    let spec = SubAgentSpec::new("test", "desc", "prompt")
        .with_tools(vec!["read_file".into(), "search".into()]);
    assert!(spec.has_tool_restriction());
    assert_eq!(spec.tools.len(), 2);
}

#[test]
fn test_subagent_spec_with_model() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_model("gpt-4");
    assert_eq!(spec.model.as_deref(), Some("gpt-4"));
}

#[test]
fn test_subagent_spec_with_max_steps() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_max_steps(50);
    assert_eq!(spec.max_steps, Some(50));
}

#[test]
fn test_subagent_spec_with_hidden() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_hidden(true);
    assert!(spec.hidden);
}

#[test]
fn test_subagent_spec_with_temperature() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_temperature(0.3);
    assert_eq!(spec.temperature, Some(0.3));
}

#[test]
fn test_with_top_p() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_top_p(0.9);
    assert_eq!(spec.top_p, Some(0.9));
}

#[test]
fn test_top_p_serde() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_top_p(0.95);
    let json = serde_json::to_string(&spec).unwrap();
    let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.top_p, Some(0.95));
}

#[test]
fn test_with_color() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_color("#38A3EE");
    assert_eq!(spec.color.as_deref(), Some("#38A3EE"));
}

#[test]
fn test_color_serde() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_color("#FF0000");
    let json = serde_json::to_string(&spec).unwrap();
    assert!(json.contains("#FF0000"));
    let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.color.as_deref(), Some("#FF0000"));
}

#[test]
fn test_color_skipped_when_none() {
    let spec = SubAgentSpec::new("test", "desc", "prompt");
    assert!(spec.color.is_none());
    let json = serde_json::to_string(&spec).unwrap();
    assert!(!json.contains("color"));
}

#[test]
fn test_with_max_tokens() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_max_tokens(8192);
    assert_eq!(spec.max_tokens, Some(8192));
}

#[test]
fn test_max_tokens_default_none() {
    let spec = SubAgentSpec::new("test", "desc", "prompt");
    assert!(spec.max_tokens.is_none());
}

#[test]
fn test_max_tokens_serde() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_max_tokens(16384);
    let json = serde_json::to_string(&spec).unwrap();
    let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.max_tokens, Some(16384));
}

#[test]
fn test_evaluate_permission_blanket_action() {
    let mut perms = HashMap::new();
    perms.insert(
        "bash".to_string(),
        PermissionRule::Action(PermissionAction::Deny),
    );

    let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

    assert_eq!(
        spec.evaluate_permission("bash", "anything"),
        Some(PermissionAction::Deny)
    );
    assert_eq!(
        spec.evaluate_permission("read_file", "anything"),
        None // No rule for read_file
    );
}

#[test]
fn test_evaluate_permission_wildcard_tool() {
    let mut perms = HashMap::new();
    perms.insert(
        "*".to_string(),
        PermissionRule::Action(PermissionAction::Ask),
    );

    let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

    assert_eq!(
        spec.evaluate_permission("bash", "anything"),
        Some(PermissionAction::Ask)
    );
    assert_eq!(
        spec.evaluate_permission("read_file", "anything"),
        Some(PermissionAction::Ask)
    );
}

#[test]
fn test_evaluate_permission_pattern_matching() {
    let mut patterns = HashMap::new();
    patterns.insert("*".to_string(), PermissionAction::Ask);
    patterns.insert("git *".to_string(), PermissionAction::Allow);
    patterns.insert("rm -rf *".to_string(), PermissionAction::Deny);

    let mut perms = HashMap::new();
    perms.insert("bash".to_string(), PermissionRule::Patterns(patterns));

    let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

    assert_eq!(
        spec.evaluate_permission("bash", "git status"),
        Some(PermissionAction::Allow)
    );
    assert_eq!(
        spec.evaluate_permission("bash", "rm -rf /"),
        Some(PermissionAction::Deny)
    );
    assert_eq!(
        spec.evaluate_permission("bash", "npm install"),
        Some(PermissionAction::Ask)
    );
}

#[test]
fn test_evaluate_permission_no_rules() {
    let spec = SubAgentSpec::new("test", "desc", "prompt");
    assert_eq!(spec.evaluate_permission("bash", "anything"), None);
}

#[test]
fn test_disabled_tools_blanket_deny() {
    let mut perms = HashMap::new();
    perms.insert(
        "edit".to_string(),
        PermissionRule::Action(PermissionAction::Deny),
    );
    perms.insert(
        "bash".to_string(),
        PermissionRule::Action(PermissionAction::Allow),
    );

    let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

    let disabled = spec.disabled_tools(&["edit", "bash", "read_file"]);
    assert_eq!(disabled, vec!["edit"]);
}

#[test]
fn test_disabled_tools_pattern_deny_not_blanket() {
    // Pattern-specific deny should NOT disable the tool entirely.
    let mut patterns = HashMap::new();
    patterns.insert("rm *".to_string(), PermissionAction::Deny);
    patterns.insert("*".to_string(), PermissionAction::Allow);

    let mut perms = HashMap::new();
    perms.insert("bash".to_string(), PermissionRule::Patterns(patterns));

    let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

    let disabled = spec.disabled_tools(&["bash"]);
    assert!(
        disabled.is_empty(),
        "Pattern-specific deny should not disable tool"
    );
}

#[test]
fn test_disable_field() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_disable(true);
    assert!(spec.disable);

    let spec2 = SubAgentSpec::new("test", "desc", "prompt");
    assert!(!spec2.disable);
}

#[test]
fn test_disable_serde() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_disable(true);
    let json = serde_json::to_string(&spec).unwrap();
    let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
    assert!(restored.disable);
}
