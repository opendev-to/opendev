use super::*;

#[test]
fn test_hook_event_roundtrip() {
    for event in HookEvent::ALL {
        let s = event.as_str();
        let parsed = HookEvent::from_config_str(s);
        assert_eq!(parsed, Some(*event), "roundtrip failed for {s}");
    }
}

#[test]
fn test_hook_event_from_invalid() {
    assert_eq!(HookEvent::from_config_str("NotAnEvent"), None);
    assert_eq!(HookEvent::from_config_str(""), None);
}

#[test]
fn test_hook_command_timeout_clamping() {
    let cmd = HookCommand::with_timeout("echo hi", 0);
    assert_eq!(cmd.effective_timeout(), 1);

    let cmd = HookCommand::with_timeout("echo hi", 9999);
    assert_eq!(cmd.effective_timeout(), 600);

    let cmd = HookCommand::with_timeout("echo hi", 30);
    assert_eq!(cmd.effective_timeout(), 30);
}

#[test]
fn test_matcher_catch_all() {
    let m = HookMatcher::catch_all(vec![HookCommand::new("echo test")]);
    assert!(m.matches(None));
    assert!(m.matches(Some("anything")));
    assert!(m.matches(Some("bash")));
}

#[test]
fn test_matcher_with_regex() {
    let m = HookMatcher::with_pattern(r"^(bash|edit)$", vec![HookCommand::new("echo test")]);
    assert!(m.matches(Some("bash")));
    assert!(m.matches(Some("edit")));
    assert!(!m.matches(Some("read")));
    // None value always matches
    assert!(m.matches(None));
}

#[test]
fn test_matcher_invalid_regex_falls_back_to_exact() {
    let m = HookMatcher::with_pattern(r"[invalid", vec![HookCommand::new("echo test")]);
    // Invalid regex can't compile, falls back to exact match
    assert!(!m.matches(Some("anything")));
    assert!(m.matches(Some("[invalid")));
}

#[test]
fn test_hook_config_deserialize() {
    let json = r#"{
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "bash",
                    "hooks": [
                        { "command": "echo pre-bash", "timeout": 10 }
                    ]
                }
            ],
            "UnknownEvent": [
                {
                    "hooks": [
                        { "command": "echo unknown" }
                    ]
                }
            ]
        }
    }"#;
    let mut config: HookConfig = serde_json::from_str(json).unwrap();
    config.compile_all();
    config.strip_unknown_events();

    assert!(config.has_hooks_for(HookEvent::PreToolUse));
    assert!(!config.has_hooks_for(HookEvent::PostToolUse));

    let matchers = config.get_matchers(HookEvent::PreToolUse);
    assert_eq!(matchers.len(), 1);
    assert!(matchers[0].matches(Some("bash")));
    assert!(!matchers[0].matches(Some("edit")));
}

#[test]
fn test_hook_config_add_matcher() {
    let mut config = HookConfig::empty();
    config.add_matcher(
        HookEvent::PostToolUse,
        HookMatcher::catch_all(vec![HookCommand::new("echo done")]),
    );
    assert!(config.has_hooks_for(HookEvent::PostToolUse));
    assert!(!config.has_hooks_for(HookEvent::PreToolUse));
}

#[test]
fn test_hook_event_classification() {
    assert!(HookEvent::PreToolUse.is_tool_event());
    assert!(HookEvent::PostToolUse.is_tool_event());
    assert!(HookEvent::PostToolUseFailure.is_tool_event());
    assert!(!HookEvent::SessionStart.is_tool_event());

    assert!(HookEvent::SubagentStart.is_subagent_event());
    assert!(HookEvent::SubagentStop.is_subagent_event());
    assert!(!HookEvent::PreToolUse.is_subagent_event());
}
