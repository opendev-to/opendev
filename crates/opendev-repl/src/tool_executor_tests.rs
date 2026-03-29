use super::*;

#[test]
fn test_format_result_success() {
    let result = ToolExecutionResult {
        tool_name: "read_file".to_string(),
        success: true,
        output: Some("file contents here".to_string()),
        error: None,
        duration_ms: 42,
    };
    let formatted = ToolExecutor::format_result(&result);
    assert!(formatted.contains("read_file"));
    assert!(formatted.contains("42ms"));
    assert!(formatted.contains("file contents here"));
}

#[test]
fn test_format_result_failure() {
    let result = ToolExecutionResult {
        tool_name: "write_file".to_string(),
        success: false,
        output: None,
        error: Some("permission denied".to_string()),
        duration_ms: 5,
    };
    let formatted = ToolExecutor::format_result(&result);
    assert!(formatted.contains("FAILED"));
    assert!(formatted.contains("permission denied"));
}
