use super::*;
use opendev_models::message::ToolCall;
use std::collections::HashMap;

fn make_tool_call(
    name: &str,
    result: Option<serde_json::Value>,
    error: Option<String>,
) -> ToolCall {
    ToolCall {
        id: "test-id".to_string(),
        name: name.to_string(),
        parameters: HashMap::new(),
        result,
        result_summary: None,
        timestamp: chrono::Utc::now(),
        approved: true,
        error,
        nested_tool_calls: Vec::new(),
    }
}

#[test]
fn test_from_model_string_result() {
    let tc = make_tool_call(
        "bash",
        Some(serde_json::Value::String("hello\nworld".to_string())),
        None,
    );
    let dtc = DisplayToolCall::from_model(&tc);
    assert_eq!(dtc.result_lines, vec!["hello", "world"]);
    assert!(dtc.success);
}

#[test]
fn test_from_model_json_result() {
    let val = serde_json::json!({"key": "value"});
    let tc = make_tool_call("bash", Some(val), None);
    let dtc = DisplayToolCall::from_model(&tc);
    assert!(!dtc.result_lines.is_empty());
    assert!(dtc.result_lines.join("\n").contains("\"key\""));
}

#[test]
fn test_from_model_50_line_cap() {
    let long_text = (0..100)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let tc = make_tool_call("bash", Some(serde_json::Value::String(long_text)), None);
    let dtc = DisplayToolCall::from_model(&tc);
    assert_eq!(dtc.result_lines.len(), 50);
}

#[test]
fn test_from_model_short_result_not_collapsed() {
    let tc = make_tool_call(
        "bash",
        Some(serde_json::Value::String("a\nb\nc".to_string())),
        None,
    );
    let dtc = DisplayToolCall::from_model(&tc);
    assert!(!dtc.collapsed);
}

#[test]
fn test_from_model_long_result_collapsed() {
    let text = (0..10)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let tc = make_tool_call("bash", Some(serde_json::Value::String(text)), None);
    let dtc = DisplayToolCall::from_model(&tc);
    assert!(dtc.collapsed);
}

#[test]
fn test_from_model_file_read_always_collapsed() {
    let tc = make_tool_call(
        "read_file",
        Some(serde_json::Value::String("short".to_string())),
        None,
    );
    let dtc = DisplayToolCall::from_model(&tc);
    assert!(dtc.collapsed);
}

#[test]
fn test_from_model_diff_tool_never_collapsed() {
    let text = (0..10)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let tc = make_tool_call("edit_file", Some(serde_json::Value::String(text)), None);
    let dtc = DisplayToolCall::from_model(&tc);
    assert!(!dtc.collapsed);
}

#[test]
fn test_from_model_error_maps_to_failure() {
    let tc = make_tool_call("bash", None, Some("command failed".to_string()));
    let dtc = DisplayToolCall::from_model(&tc);
    assert!(!dtc.success);
}

#[test]
fn test_from_model_nested_calls() {
    let mut tc = make_tool_call("spawn_subagent", None, None);
    tc.nested_tool_calls = vec![make_tool_call(
        "bash",
        Some(serde_json::Value::String("nested output".to_string())),
        None,
    )];
    let dtc = DisplayToolCall::from_model(&tc);
    assert_eq!(dtc.nested_calls.len(), 1);
    assert_eq!(dtc.nested_calls[0].name, "bash");
    assert_eq!(dtc.nested_calls[0].result_lines, vec!["nested output"]);
}

#[test]
fn test_from_model_no_result() {
    let tc = make_tool_call("bash", None, None);
    let dtc = DisplayToolCall::from_model(&tc);
    assert!(dtc.result_lines.is_empty());
    assert!(!dtc.collapsed);
}
