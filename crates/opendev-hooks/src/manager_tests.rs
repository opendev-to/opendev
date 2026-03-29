use super::*;
use crate::models::{HookCommand, HookMatcher};
use serde_json::json;

fn make_config_with_echo(event: HookEvent, pattern: Option<&str>, cmd: &str) -> HookConfig {
    let mut config = HookConfig::empty();
    let matcher = match pattern {
        Some(p) => HookMatcher::with_pattern(p, vec![HookCommand::new(cmd)]),
        None => HookMatcher::catch_all(vec![HookCommand::new(cmd)]),
    };
    config.add_matcher(event, matcher);
    config
}

#[test]
fn test_noop_manager() {
    let manager = HookManager::noop();
    assert!(!manager.has_hooks_for(HookEvent::PreToolUse));
    assert!(!manager.has_hooks_for(HookEvent::SessionStart));
}

#[test]
fn test_has_hooks_for() {
    let config = make_config_with_echo(HookEvent::PreToolUse, None, "echo test");
    let manager = HookManager::new(config, "sess-1", "/tmp");
    assert!(manager.has_hooks_for(HookEvent::PreToolUse));
    assert!(!manager.has_hooks_for(HookEvent::PostToolUse));
}

#[tokio::test]
async fn test_run_hooks_no_matchers() {
    let manager = HookManager::noop();
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;
    assert!(outcome.allowed());
    assert!(outcome.results.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn test_run_hooks_success() {
    let config = make_config_with_echo(HookEvent::PreToolUse, None, "echo ok");
    let manager = HookManager::new(config, "sess-1", "/tmp");
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;
    assert!(outcome.allowed());
    assert_eq!(outcome.results.len(), 1);
    assert!(outcome.results[0].success());
}

#[cfg(unix)]
#[tokio::test]
async fn test_run_hooks_blocked() {
    let config = make_config_with_echo(
        HookEvent::PreToolUse,
        None,
        r#"echo '{"reason":"dangerous"}' && exit 2"#,
    );
    let manager = HookManager::new(config, "sess-1", "/tmp");
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;
    assert!(outcome.blocked);
    assert_eq!(outcome.block_reason, "dangerous");
}

#[cfg(unix)]
#[tokio::test]
async fn test_run_hooks_block_stderr_fallback() {
    let config = make_config_with_echo(
        HookEvent::PreToolUse,
        None,
        "echo 'not blocked' >&2; exit 2",
    );
    let manager = HookManager::new(config, "sess-1", "/tmp");
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;
    assert!(outcome.blocked);
    assert_eq!(outcome.block_reason, "not blocked");
}

#[tokio::test]
async fn test_run_hooks_matcher_filters() {
    let config = make_config_with_echo(HookEvent::PreToolUse, Some(r"^bash$"), "exit 2");
    let manager = HookManager::new(config, "sess-1", "/tmp");

    // "bash" matches the pattern -> blocked
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;
    assert!(outcome.blocked);

    // "edit" does not match -> allowed, no hooks run
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("edit"), None)
        .await;
    assert!(outcome.allowed());
    assert!(outcome.results.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn test_run_hooks_additional_context() {
    let config = make_config_with_echo(
        HookEvent::PostToolUse,
        None,
        r#"echo '{"additionalContext":"extra info"}'"#,
    );
    let manager = HookManager::new(config, "sess-1", "/tmp");
    let outcome = manager
        .run_hooks(HookEvent::PostToolUse, Some("bash"), None)
        .await;
    assert!(outcome.allowed());
    assert_eq!(outcome.additional_context.as_deref(), Some("extra info"));
}

#[cfg(unix)]
#[tokio::test]
async fn test_run_hooks_permission_decision() {
    let config = make_config_with_echo(
        HookEvent::PreToolUse,
        None,
        r#"echo '{"permissionDecision":"allow"}'"#,
    );
    let manager = HookManager::new(config, "sess-1", "/tmp");
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;
    assert!(outcome.allowed());
    assert_eq!(outcome.permission_decision.as_deref(), Some("allow"));
}

#[cfg(unix)]
#[tokio::test]
async fn test_run_hooks_updated_input() {
    let config = make_config_with_echo(
        HookEvent::PreToolUse,
        None,
        r#"echo '{"updatedInput":{"command":"ls -la"}}'"#,
    );
    let manager = HookManager::new(config, "sess-1", "/tmp");
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;
    assert!(outcome.allowed());
    let updated = outcome.updated_input.unwrap();
    assert_eq!(updated["command"], "ls -la");
}

#[tokio::test]
async fn test_run_hooks_multiple_commands_short_circuit() {
    // First command blocks -> second should not run
    let mut config = HookConfig::empty();
    let matcher = HookMatcher::catch_all(vec![
        HookCommand::new("exit 2"),
        HookCommand::new("echo should-not-run"),
    ]);
    config.add_matcher(HookEvent::PreToolUse, matcher);

    let manager = HookManager::new(config, "sess-1", "/tmp");
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;
    assert!(outcome.blocked);
    // Only one result because we short-circuited
    assert_eq!(outcome.results.len(), 1);
}

#[tokio::test]
async fn test_run_hooks_error_continues() {
    // A failing command (non-zero, non-2 exit) should not block
    let config = make_config_with_echo(HookEvent::PostToolUse, None, "exit 1");
    let manager = HookManager::new(config, "sess-1", "/tmp");
    let outcome = manager
        .run_hooks(HookEvent::PostToolUse, Some("bash"), None)
        .await;
    assert!(outcome.allowed());
    assert_eq!(outcome.results.len(), 1);
    assert!(!outcome.results[0].success());
}

#[tokio::test]
async fn test_build_stdin_tool_event() {
    let config = make_config_with_echo(
        HookEvent::PreToolUse,
        None,
        "cat", // echo stdin back
    );
    let manager = HookManager::new(config, "sess-42", "/home/user");

    let event_data = json!({
        "tool_input": {"command": "ls"},
        "extra_field": "value"
    });

    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), Some(&event_data))
        .await;

    assert!(outcome.allowed());
    let stdout = &outcome.results[0].stdout;
    let parsed: Value = serde_json::from_str(stdout).unwrap();

    assert_eq!(parsed["session_id"], "sess-42");
    assert_eq!(parsed["cwd"], "/home/user");
    assert_eq!(parsed["hook_event_name"], "PreToolUse");
    assert_eq!(parsed["tool_name"], "bash");
    assert_eq!(parsed["tool_input"]["command"], "ls");
    assert_eq!(parsed["extra_field"], "value");
}

#[tokio::test]
async fn test_build_stdin_session_start() {
    let config = make_config_with_echo(HookEvent::SessionStart, None, "cat");
    let manager = HookManager::new(config, "sess-1", "/tmp");

    let outcome = manager
        .run_hooks(HookEvent::SessionStart, Some("resume"), None)
        .await;

    let stdout = &outcome.results[0].stdout;
    let parsed: Value = serde_json::from_str(stdout).unwrap();
    assert_eq!(parsed["startup_type"], "resume");
    assert_eq!(parsed["hook_event_name"], "SessionStart");
}

#[tokio::test]
async fn test_build_stdin_session_start_default() {
    let config = make_config_with_echo(HookEvent::SessionStart, None, "cat");
    let manager = HookManager::new(config, "sess-1", "/tmp");

    let outcome = manager.run_hooks(HookEvent::SessionStart, None, None).await;

    let stdout = &outcome.results[0].stdout;
    let parsed: Value = serde_json::from_str(stdout).unwrap();
    // Default startup_type when match_value is None
    assert_eq!(parsed["startup_type"], "startup");
}

#[tokio::test]
async fn test_build_stdin_subagent_event() {
    let config = make_config_with_echo(HookEvent::SubagentStart, None, "cat");
    let manager = HookManager::new(config, "sess-1", "/tmp");

    let outcome = manager
        .run_hooks(HookEvent::SubagentStart, Some("code_explorer"), None)
        .await;

    let stdout = &outcome.results[0].stdout;
    let parsed: Value = serde_json::from_str(stdout).unwrap();
    assert_eq!(parsed["agent_type"], "code_explorer");
}

#[tokio::test]
async fn test_build_stdin_pre_compact() {
    let config = make_config_with_echo(HookEvent::PreCompact, None, "cat");
    let manager = HookManager::new(config, "sess-1", "/tmp");

    let outcome = manager
        .run_hooks(HookEvent::PreCompact, Some("manual"), None)
        .await;

    let stdout = &outcome.results[0].stdout;
    let parsed: Value = serde_json::from_str(stdout).unwrap();
    assert_eq!(parsed["trigger"], "manual");
}

#[tokio::test]
async fn test_multiple_matchers_both_run() {
    let mut config = HookConfig::empty();
    // Two catch-all matchers, each with one command
    config.add_matcher(
        HookEvent::PostToolUse,
        HookMatcher::catch_all(vec![HookCommand::new("echo first")]),
    );
    config.add_matcher(
        HookEvent::PostToolUse,
        HookMatcher::catch_all(vec![HookCommand::new("echo second")]),
    );

    let manager = HookManager::new(config, "sess-1", "/tmp");
    let outcome = manager
        .run_hooks(HookEvent::PostToolUse, Some("bash"), None)
        .await;

    assert!(outcome.allowed());
    assert_eq!(outcome.results.len(), 2);
    assert_eq!(outcome.results[0].stdout.trim(), "first");
    assert_eq!(outcome.results[1].stdout.trim(), "second");
}
