//! Integration tests for the hooks system.
//!
//! Tests the full hook lifecycle: event matching, subprocess execution,
//! exit code protocol (0=success, 2=block), timeout enforcement,
//! JSON output parsing, and multi-matcher orchestration.
//!
//! These tests rely on Unix shell syntax (sh -c, single quotes, >&2)
//! and are skipped on non-Unix platforms.
#![cfg(unix)]

use serde_json::json;

use opendev_hooks::{HookCommand, HookConfig, HookEvent, HookManager, HookMatcher};

fn make_config(event: HookEvent, pattern: Option<&str>, cmd: &str) -> HookConfig {
    let mut config = HookConfig::empty();
    let matcher = match pattern {
        Some(p) => HookMatcher::with_pattern(p, vec![HookCommand::new(cmd)]),
        None => HookMatcher::catch_all(vec![HookCommand::new(cmd)]),
    };
    config.add_matcher(event, matcher);
    config
}

// ========================================================================
// Pre-tool hook blocking (exit 2)
// ========================================================================

/// A PreToolUse hook that exits with code 2 should block the operation
/// and capture the JSON reason from stdout.
#[tokio::test]
async fn pre_tool_hook_blocks_with_exit_2_and_json_reason() {
    let config = make_config(
        HookEvent::PreToolUse,
        None,
        r#"echo '{"reason":"security policy violation","decision":"deny"}' && exit 2"#,
    );
    let manager = HookManager::new(config, "sess-block", "/tmp");

    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;

    assert!(outcome.blocked, "exit code 2 should block");
    assert!(!outcome.allowed());
    assert_eq!(outcome.block_reason, "security policy violation");
    assert_eq!(outcome.decision.as_deref(), Some("deny"));
}

/// Pre-tool hook with regex matcher should only block matching tools.
#[tokio::test]
async fn pre_tool_hook_regex_matcher_selectively_blocks() {
    let config = make_config(
        HookEvent::PreToolUse,
        Some(r"^(bash|write_file)$"),
        "exit 2",
    );
    let manager = HookManager::new(config, "sess-regex", "/tmp");

    // bash matches -> blocked
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;
    assert!(outcome.blocked);

    // write_file matches -> blocked
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("write_file"), None)
        .await;
    assert!(outcome.blocked);

    // read_file does NOT match -> allowed
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("read_file"), None)
        .await;
    assert!(outcome.allowed());
    assert!(
        outcome.results.is_empty(),
        "no hooks should run for non-matching tool"
    );
}

// ========================================================================
// Post-tool hook with JSON output
// ========================================================================

/// A PostToolUse hook that outputs JSON with additionalContext should
/// inject that context into the outcome.
#[tokio::test]
async fn post_tool_hook_injects_additional_context() {
    let config = make_config(
        HookEvent::PostToolUse,
        None,
        r#"echo '{"additionalContext":"Remember to run tests after editing","decision":"ok"}'"#,
    );
    let manager = HookManager::new(config, "sess-ctx", "/tmp");

    let outcome = manager
        .run_hooks(HookEvent::PostToolUse, Some("edit_file"), None)
        .await;

    assert!(outcome.allowed());
    assert_eq!(
        outcome.additional_context.as_deref(),
        Some("Remember to run tests after editing")
    );
    assert_eq!(outcome.decision.as_deref(), Some("ok"));
}

/// A PostToolUse hook can provide updatedInput to modify tool arguments.
#[tokio::test]
async fn post_tool_hook_provides_updated_input() {
    let config = make_config(
        HookEvent::PreToolUse,
        None,
        r#"echo '{"updatedInput":{"command":"ls -la --color=never"}}'"#,
    );
    let manager = HookManager::new(config, "sess-input", "/tmp");

    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;

    assert!(outcome.allowed());
    let updated = outcome.updated_input.unwrap();
    assert_eq!(updated["command"], "ls -la --color=never");
}

// ========================================================================
// Timeout enforcement
// ========================================================================

/// A hook that exceeds its timeout should be killed and not block.
#[tokio::test]
async fn hook_timeout_kills_long_running_command() {
    let mut config = HookConfig::empty();
    let matcher = HookMatcher::catch_all(vec![HookCommand::with_timeout("sleep 60", 1)]);
    config.add_matcher(HookEvent::PreToolUse, matcher);

    let manager = HookManager::new(config, "sess-timeout", "/tmp");

    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;

    // Timeout should not block (exit code from kill is not 2)
    assert!(outcome.allowed());
    assert_eq!(outcome.results.len(), 1);
    assert!(outcome.results[0].timed_out);
    assert!(!outcome.results[0].success());
}

// ========================================================================
// Event matching with regex patterns
// ========================================================================

/// Hook config deserialized from JSON with regex patterns, compiled,
/// and used for event matching.
#[tokio::test]
async fn hook_config_from_json_with_regex_matching() {
    let json_config = r#"{
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "^(bash|edit_file)$",
                    "hooks": [
                        { "command": "echo matched", "timeout": 5 }
                    ]
                }
            ],
            "PostToolUse": [
                {
                    "hooks": [
                        { "command": "echo post" }
                    ]
                }
            ]
        }
    }"#;

    let mut config: HookConfig = serde_json::from_str(json_config).unwrap();
    config.compile_all();
    config.strip_unknown_events();

    let manager = HookManager::new(config, "sess-json", "/tmp");

    // PreToolUse with bash -> matcher fires
    assert!(manager.has_hooks_for(HookEvent::PreToolUse));
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;
    assert!(outcome.allowed());
    assert_eq!(outcome.results.len(), 1);
    assert_eq!(outcome.results[0].stdout.trim(), "matched");

    // PreToolUse with read_file -> matcher does NOT fire
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("read_file"), None)
        .await;
    assert!(outcome.results.is_empty());

    // PostToolUse catch-all fires for everything
    assert!(manager.has_hooks_for(HookEvent::PostToolUse));
    let outcome = manager
        .run_hooks(HookEvent::PostToolUse, Some("anything"), None)
        .await;
    assert_eq!(outcome.results.len(), 1);
    assert_eq!(outcome.results[0].stdout.trim(), "post");
}

// ========================================================================
// Short-circuit on block
// ========================================================================

/// When a hook blocks, subsequent hooks should NOT execute.
#[tokio::test]
async fn multiple_hooks_short_circuit_on_block() {
    let mut config = HookConfig::empty();
    // First hook blocks
    config.add_matcher(
        HookEvent::PreToolUse,
        HookMatcher::catch_all(vec![HookCommand::new(
            r#"echo '{"reason":"first blocks"}' && exit 2"#,
        )]),
    );
    // Second hook would succeed (but should never run)
    config.add_matcher(
        HookEvent::PreToolUse,
        HookMatcher::catch_all(vec![HookCommand::new("echo second-should-not-run")]),
    );

    let manager = HookManager::new(config, "sess-sc", "/tmp");
    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), None)
        .await;

    assert!(outcome.blocked);
    assert_eq!(outcome.block_reason, "first blocks");
    // Only 1 result since we short-circuited
    assert_eq!(outcome.results.len(), 1);
}

// ========================================================================
// Stdin payload structure
// ========================================================================

/// The stdin payload for tool events should include session_id, cwd,
/// hook_event_name, tool_name, and merged event data.
#[tokio::test]
async fn stdin_payload_includes_all_fields() {
    let config = make_config(HookEvent::PreToolUse, None, "cat");
    let manager = HookManager::new(config, "sess-stdin", "/workspace/project");

    let event_data = json!({
        "tool_input": {"command": "cargo test"},
        "custom_field": 42
    });

    let outcome = manager
        .run_hooks(HookEvent::PreToolUse, Some("bash"), Some(&event_data))
        .await;

    let stdout = &outcome.results[0].stdout;
    let parsed: serde_json::Value = serde_json::from_str(stdout).unwrap();

    assert_eq!(parsed["session_id"], "sess-stdin");
    assert_eq!(parsed["cwd"], "/workspace/project");
    assert_eq!(parsed["hook_event_name"], "PreToolUse");
    assert_eq!(parsed["tool_name"], "bash");
    assert_eq!(parsed["tool_input"]["command"], "cargo test");
    assert_eq!(parsed["custom_field"], 42);
}

// ========================================================================
// Hook event coverage
// ========================================================================

/// All 10 hook events should roundtrip through as_str/from_config_str.
#[test]
fn all_hook_events_roundtrip() {
    assert_eq!(HookEvent::ALL.len(), 10);
    for event in HookEvent::ALL {
        let s = event.as_str();
        let parsed = HookEvent::from_config_str(s);
        assert_eq!(parsed, Some(*event), "roundtrip failed for {s}");
    }
}

/// Tool events are correctly classified.
#[test]
fn tool_events_classified_correctly() {
    let tool_events = [
        HookEvent::PreToolUse,
        HookEvent::PostToolUse,
        HookEvent::PostToolUseFailure,
    ];
    for event in &tool_events {
        assert!(event.is_tool_event());
        assert!(!event.is_subagent_event());
    }
}

/// Non-zero non-2 exit codes should log errors but NOT block.
#[tokio::test]
async fn hook_error_exit_code_does_not_block() {
    let config = make_config(HookEvent::PostToolUse, None, "exit 1");
    let manager = HookManager::new(config, "sess-err", "/tmp");

    let outcome = manager
        .run_hooks(HookEvent::PostToolUse, Some("bash"), None)
        .await;

    assert!(outcome.allowed(), "exit 1 should NOT block");
    assert_eq!(outcome.results.len(), 1);
    assert!(!outcome.results[0].success());
    assert_eq!(outcome.results[0].exit_code, 1);
}
