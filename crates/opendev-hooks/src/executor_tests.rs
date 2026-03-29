use super::*;

#[test]
fn test_hook_result_success() {
    let r = HookResult::default();
    assert!(r.success());
    assert!(!r.should_block());
}

#[test]
fn test_hook_result_block() {
    let r = HookResult {
        exit_code: 2,
        ..Default::default()
    };
    assert!(!r.success());
    assert!(r.should_block());
}

#[test]
fn test_hook_result_timeout() {
    let r = HookResult {
        exit_code: 1,
        timed_out: true,
        error: Some("timed out".into()),
        ..Default::default()
    };
    assert!(!r.success());
    assert!(!r.should_block());
}

#[test]
fn test_hook_result_error() {
    let r = HookResult {
        exit_code: 0,
        error: Some("oops".into()),
        ..Default::default()
    };
    assert!(!r.success());
}

#[test]
fn test_parse_json_output_valid() {
    let r = HookResult {
        stdout: r#"{"reason": "blocked", "decision": "deny"}"#.into(),
        ..Default::default()
    };
    let parsed = r.parse_json_output();
    assert_eq!(
        parsed.get("reason").and_then(|v| v.as_str()),
        Some("blocked")
    );
    assert_eq!(
        parsed.get("decision").and_then(|v| v.as_str()),
        Some("deny")
    );
}

#[test]
fn test_parse_json_output_empty() {
    let r = HookResult::default();
    assert!(r.parse_json_output().is_empty());
}

#[test]
fn test_parse_json_output_invalid() {
    let r = HookResult {
        stdout: "not json".into(),
        ..Default::default()
    };
    assert!(r.parse_json_output().is_empty());
}

#[tokio::test]
async fn test_executor_echo_command() {
    let executor = HookExecutor::new();
    let cmd = HookCommand::new("echo hello");
    let stdin = serde_json::json!({"test": true});

    let result = executor.execute(&cmd, &stdin).await;
    assert!(result.success());
    assert_eq!(result.stdout.trim(), "hello");
}

#[tokio::test]
async fn test_executor_reads_stdin() {
    let executor = HookExecutor::new();
    let cmd = HookCommand::new("cat");
    let stdin = serde_json::json!({"key": "value"});

    let result = executor.execute(&cmd, &stdin).await;
    assert!(result.success());
    let parsed: serde_json::Value = serde_json::from_str(result.stdout.trim()).unwrap();
    assert_eq!(parsed["key"], "value");
}

#[tokio::test]
async fn test_executor_exit_code_2_blocks() {
    let executor = HookExecutor::new();
    let cmd = HookCommand::new("exit 2");
    let stdin = serde_json::json!({});

    let result = executor.execute(&cmd, &stdin).await;
    assert!(result.should_block());
    assert!(!result.success());
}

#[tokio::test]
async fn test_executor_nonzero_exit() {
    let executor = HookExecutor::new();
    let cmd = HookCommand::new("exit 1");
    let stdin = serde_json::json!({});

    let result = executor.execute(&cmd, &stdin).await;
    assert!(!result.success());
    assert!(!result.should_block());
    assert_eq!(result.exit_code, 1);
}

#[tokio::test]
async fn test_executor_timeout() {
    let executor = HookExecutor::new();
    let cmd = HookCommand::with_timeout("sleep 60", 1);
    let stdin = serde_json::json!({});

    let result = executor.execute(&cmd, &stdin).await;
    assert!(result.timed_out);
    assert!(!result.success());
}

#[tokio::test]
async fn test_executor_invalid_command() {
    let executor = HookExecutor::new();
    let cmd = HookCommand::new("__nonexistent_command_xyz_12345__");
    let stdin = serde_json::json!({});

    let result = executor.execute(&cmd, &stdin).await;
    // The shell will report an error (command not found) with non-zero exit
    assert!(!result.success());
}

#[tokio::test]
async fn test_executor_captures_stderr() {
    let executor = HookExecutor::new();
    let cmd = HookCommand::new("echo err >&2");
    let stdin = serde_json::json!({});

    let result = executor.execute(&cmd, &stdin).await;
    assert!(result.success());
    assert_eq!(result.stderr.trim(), "err");
}

#[cfg(unix)]
#[tokio::test]
async fn test_executor_json_stdout() {
    let executor = HookExecutor::new();
    let cmd = HookCommand::new(r#"echo '{"additionalContext":"extra info"}'"#);
    let stdin = serde_json::json!({});

    let result = executor.execute(&cmd, &stdin).await;
    assert!(result.success());
    let parsed = result.parse_json_output();
    assert_eq!(
        parsed.get("additionalContext").and_then(|v| v.as_str()),
        Some("extra info")
    );
}
