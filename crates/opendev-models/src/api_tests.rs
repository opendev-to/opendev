use super::*;
use chrono::Utc;

#[test]
fn test_tool_call_to_response() {
    let tc = ToolCall {
        id: "tc-1".to_string(),
        name: "read_file".to_string(),
        parameters: HashMap::new(),
        result: Some(serde_json::Value::String("file contents".to_string())),
        result_summary: Some("Read 10 lines".to_string()),
        timestamp: Utc::now(),
        approved: true,
        error: None,
        nested_tool_calls: vec![],
    };

    let response = tool_call_to_response(&tc);
    assert_eq!(response.id, "tc-1");
    assert_eq!(response.name, "read_file");
    assert_eq!(response.result.as_deref(), Some("file contents"));
    assert_eq!(response.result_summary.as_deref(), Some("Read 10 lines"));
    assert!(response.nested_tool_calls.is_none());
}

#[test]
fn test_tool_call_to_response_with_nested() {
    let nested = ToolCall {
        id: "nested-1".to_string(),
        name: "bash".to_string(),
        parameters: HashMap::new(),
        result: Some(serde_json::json!({"exit_code": 0})),
        result_summary: None,
        timestamp: Utc::now(),
        approved: true,
        error: None,
        nested_tool_calls: vec![],
    };

    let tc = ToolCall {
        id: "tc-1".to_string(),
        name: "agent".to_string(),
        parameters: HashMap::new(),
        result: Some(serde_json::Value::String("done".to_string())),
        result_summary: None,
        timestamp: Utc::now(),
        approved: true,
        error: None,
        nested_tool_calls: vec![nested],
    };

    let response = tool_call_to_response(&tc);
    assert!(response.nested_tool_calls.is_some());
    let nested_responses = response.nested_tool_calls.unwrap();
    assert_eq!(nested_responses.len(), 1);
    assert_eq!(nested_responses[0].name, "bash");
    // Non-string result should be serialized to JSON string
    assert!(
        nested_responses[0]
            .result
            .as_ref()
            .unwrap()
            .contains("exit_code")
    );
}

#[test]
fn test_session_response_roundtrip() {
    let resp = SessionResponse {
        id: "abc123".to_string(),
        working_dir: "/home/user/project".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-01T01:00:00Z".to_string(),
        message_count: 10,
        total_tokens: 5000,
        title: Some("Test session".to_string()),
        has_session_model: false,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: SessionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "abc123");
    assert_eq!(deserialized.message_count, 10);
}
