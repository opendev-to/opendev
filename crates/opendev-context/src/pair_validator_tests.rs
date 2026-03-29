use super::*;

fn make_msg(role: &str, content: &str) -> ApiMessage {
    let mut msg = ApiMessage::new();
    msg.insert(
        "role".to_string(),
        serde_json::Value::String(role.to_string()),
    );
    msg.insert(
        "content".to_string(),
        serde_json::Value::String(content.to_string()),
    );
    msg
}

fn make_assistant_with_tc(tc_ids: &[&str]) -> ApiMessage {
    let mut msg = ApiMessage::new();
    msg.insert(
        "role".to_string(),
        serde_json::Value::String("assistant".to_string()),
    );
    msg.insert(
        "content".to_string(),
        serde_json::Value::String(String::new()),
    );
    let tcs: Vec<serde_json::Value> = tc_ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "function": { "name": "bash", "arguments": "{}" }
            })
        })
        .collect();
    msg.insert("tool_calls".to_string(), serde_json::Value::Array(tcs));
    msg
}

fn make_tool_result(tool_call_id: &str, content: &str) -> ApiMessage {
    let mut msg = ApiMessage::new();
    msg.insert(
        "role".to_string(),
        serde_json::Value::String("tool".to_string()),
    );
    msg.insert(
        "tool_call_id".to_string(),
        serde_json::Value::String(tool_call_id.to_string()),
    );
    msg.insert(
        "content".to_string(),
        serde_json::Value::String(content.to_string()),
    );
    msg
}

#[test]
fn test_valid_messages() {
    let messages = vec![
        make_msg("user", "hello"),
        make_assistant_with_tc(&["tc-1"]),
        make_tool_result("tc-1", "result"),
        make_msg("assistant", "done"),
    ];
    let result = MessagePairValidator::validate(&messages);
    assert!(result.is_valid());
}

#[test]
fn test_missing_tool_result() {
    let messages = vec![
        make_msg("user", "hello"),
        make_assistant_with_tc(&["tc-1", "tc-2"]),
        make_tool_result("tc-1", "result"),
        // tc-2 result is missing
    ];
    let result = MessagePairValidator::validate(&messages);
    assert!(!result.is_valid());
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.violation_type == ViolationType::MissingToolResult)
    );
}

#[test]
fn test_orphaned_tool_result() {
    let messages = vec![
        make_msg("user", "hello"),
        make_msg("assistant", "thinking..."),
        make_tool_result("tc-999", "orphan result"),
    ];
    let result = MessagePairValidator::validate(&messages);
    assert!(!result.is_valid());
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.violation_type == ViolationType::OrphanedToolResult)
    );
}

#[test]
fn test_consecutive_same_role() {
    let messages = vec![make_msg("user", "hello"), make_msg("user", "hello again")];
    let result = MessagePairValidator::validate(&messages);
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.violation_type == ViolationType::ConsecutiveSameRole)
    );
}

#[test]
fn test_consecutive_tool_messages_ok() {
    // Multiple tool results in a row are valid (parallel tool execution)
    let messages = vec![
        make_msg("user", "run two things"),
        make_assistant_with_tc(&["tc-1", "tc-2"]),
        make_tool_result("tc-1", "result1"),
        make_tool_result("tc-2", "result2"),
    ];
    let result = MessagePairValidator::validate(&messages);
    // Should not have ConsecutiveSameRole for tool messages
    assert!(
        !result
            .violations
            .iter()
            .any(|v| v.violation_type == ViolationType::ConsecutiveSameRole)
    );
}

#[test]
fn test_repair_missing_tool_result() {
    let messages = vec![
        make_msg("user", "hello"),
        make_assistant_with_tc(&["tc-1"]),
        // Missing tool result for tc-1
        make_msg("user", "next"),
    ];
    let (repaired, vr) = MessagePairValidator::repair(&messages);
    assert!(vr.repaired);
    // Should have inserted synthetic result
    let has_synthetic = repaired
        .iter()
        .any(|m| m.get("content").and_then(|v| v.as_str()) == Some(SYNTHETIC_TOOL_RESULT));
    assert!(has_synthetic);
}

#[test]
fn test_repair_orphaned_tool_result() {
    let messages = vec![
        make_msg("user", "hello"),
        make_msg("assistant", "thinking"),
        make_tool_result("tc-orphan", "orphan"),
    ];
    let (repaired, vr) = MessagePairValidator::repair(&messages);
    assert!(vr.repaired);
    // Orphan should be removed
    assert_eq!(repaired.len(), 2);
}

#[test]
fn test_validate_tool_results_complete() {
    let tool_calls = vec![
        serde_json::json!({"id": "tc-1", "function": {"name": "bash", "arguments": "{}"}}),
        serde_json::json!({"id": "tc-2", "function": {"name": "read_file", "arguments": "{}"}}),
    ];
    let mut results: HashMap<String, serde_json::Value> = HashMap::new();
    results.insert("tc-1".to_string(), serde_json::json!({"output": "ok"}));
    // tc-2 is missing

    MessagePairValidator::validate_tool_results_complete(&tool_calls, &mut results);
    assert!(results.contains_key("tc-2"));
    assert_eq!(results["tc-2"]["synthetic"].as_bool(), Some(true));
}
