use super::*;

fn tool(name: &str) -> ToolCallInfo {
    ToolCallInfo {
        name: name.to_string(),
        parameters: HashMap::new(),
    }
}

fn tool_with_param(name: &str, key: &str, value: &str) -> ToolCallInfo {
    let mut params = HashMap::new();
    params.insert(key.to_string(), value.to_string());
    ToolCallInfo {
        name: name.to_string(),
        parameters: params,
    }
}

#[test]
fn test_no_reflection_for_single_read() {
    let reflector = ExecutionReflector::default();
    let result = reflector.reflect("query", &[tool("read_file")], "success");
    assert!(result.is_none());
}

#[test]
fn test_file_operation_list_then_read() {
    let reflector = ExecutionReflector::default();
    let calls = vec![tool("list_files"), tool("read_file")];
    let result = reflector.reflect("check files", &calls, "success");
    assert!(result.is_some());
    let r = result.unwrap();
    assert_eq!(r.category, "file_operations");
    assert!(r.confidence >= 0.6);
}

#[test]
fn test_file_operation_read_then_write() {
    let reflector = ExecutionReflector::default();
    let calls = vec![tool("read_file"), tool("write_file")];
    let result = reflector.reflect("update file", &calls, "success");
    assert!(result.is_some());
    assert_eq!(result.unwrap().category, "file_operations");
}

#[test]
fn test_code_navigation_search_then_read() {
    let reflector = ExecutionReflector::default();
    let calls = vec![tool("search"), tool("read_file")];
    let result = reflector.reflect("find function", &calls, "success");
    assert!(result.is_some());
    assert_eq!(result.unwrap().category, "code_navigation");
}

#[test]
fn test_multiple_reads_pattern() {
    let reflector = ExecutionReflector::default();
    let calls = vec![tool("read_file"), tool("read_file"), tool("read_file")];
    let result = reflector.reflect("understand code", &calls, "success");
    assert!(result.is_some());
    assert_eq!(result.unwrap().category, "code_navigation");
}

#[test]
fn test_testing_pattern() {
    let reflector = ExecutionReflector::default();
    let calls = vec![
        tool("write_file"),
        tool_with_param("run_command", "command", "pytest tests/"),
    ];
    let result = reflector.reflect("fix test", &calls, "success");
    assert!(result.is_some());
    assert_eq!(result.unwrap().category, "testing");
}

#[test]
fn test_shell_install_then_run() {
    let reflector = ExecutionReflector::default();
    let calls = vec![
        tool_with_param("run_command", "command", "pip install -r requirements.txt"),
        tool_with_param("run_command", "command", "python main.py"),
    ];
    let result = reflector.reflect("run app", &calls, "success");
    assert!(result.is_some());
    assert_eq!(result.unwrap().category, "shell_commands");
}

#[test]
fn test_git_status_pattern() {
    let reflector = ExecutionReflector::default();
    let calls = vec![
        tool_with_param("run_command", "command", "git status"),
        tool_with_param("run_command", "command", "git commit -m 'fix'"),
    ];
    let result = reflector.reflect("commit", &calls, "success");
    assert!(result.is_some());
    assert_eq!(result.unwrap().category, "git_operations");
}

#[test]
fn test_error_recovery_file_access() {
    let reflector = ExecutionReflector::default();
    let calls = vec![tool("read_file")];
    let result = reflector.reflect("read config", &calls, "error");
    assert!(result.is_some());
    assert_eq!(result.unwrap().category, "error_handling");
}

#[test]
fn test_error_recovery_command_failure() {
    let reflector = ExecutionReflector::default();
    let calls = vec![tool("run_command")];
    let result = reflector.reflect("build", &calls, "error");
    assert!(result.is_some());
    assert_eq!(result.unwrap().category, "error_handling");
}

#[test]
fn test_no_learning_from_empty() {
    let reflector = ExecutionReflector::default();
    let result = reflector.reflect("query", &[], "success");
    assert!(result.is_none());
}

#[test]
fn test_confidence_threshold() {
    let reflector = ExecutionReflector::new(2, 0.9); // High threshold
    let calls = vec![tool("read_file"), tool("read_file")];
    // Most patterns have confidence < 0.9, so this should fail
    let result = reflector.reflect("query", &calls, "success");
    assert!(result.is_none());
}

// ------------------------------------------------------------------ //
// score_reflection tests
// ------------------------------------------------------------------ //

#[test]
fn test_score_reflection_zero_evidence() {
    let score = score_reflection("some insight", 0, 0);
    assert_eq!(score, 0.0);
}

#[test]
fn test_score_reflection_fresh() {
    let score = score_reflection("fresh insight", 5, 0);
    // 5 * 0.95^0 = 5.0
    assert!((score - 5.0).abs() < 1e-10);
}

#[test]
fn test_score_reflection_one_day_old() {
    let score = score_reflection("day old", 1, 1);
    // 1 * 0.95^1 = 0.95
    assert!((score - 0.95).abs() < 1e-10);
}

#[test]
fn test_score_reflection_decays_over_time() {
    let fresh = score_reflection("insight", 3, 0);
    let week_old = score_reflection("insight", 3, 7);
    let month_old = score_reflection("insight", 3, 30);

    assert!(fresh > week_old, "fresh > week old");
    assert!(week_old > month_old, "week old > month old");
    assert!(month_old > 0.0, "month old still positive");
}

#[test]
fn test_score_reflection_more_evidence_higher_score() {
    let low = score_reflection("insight", 1, 5);
    let high = score_reflection("insight", 10, 5);
    assert!(high > low);
    // Both should have the same decay factor: 0.95^5
    let ratio = high / low;
    assert!((ratio - 10.0).abs() < 1e-10);
}

#[test]
fn test_score_reflection_large_age() {
    let score = score_reflection("ancient", 1, 365);
    // 0.95^365 is very small but positive
    assert!(score > 0.0);
    assert!(score < 0.001, "very old reflection should have tiny score");
}
