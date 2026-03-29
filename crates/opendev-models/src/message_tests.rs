use super::*;

#[test]
fn test_role_serialization() {
    let role = Role::User;
    let json = serde_json::to_string(&role).unwrap();
    assert_eq!(json, "\"user\"");

    let deserialized: Role = serde_json::from_str("\"assistant\"").unwrap();
    assert_eq!(deserialized, Role::Assistant);
}

#[test]
fn test_chat_message_roundtrip() {
    let msg = ChatMessage {
        role: Role::User,
        content: "Hello world".to_string(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: vec![],
        tokens: Some(3),
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    };

    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.role, Role::User);
    assert_eq!(deserialized.content, "Hello world");
    assert_eq!(deserialized.tokens, Some(3));
}

#[test]
fn test_tool_call_with_nested() {
    let nested = ToolCall {
        id: "nested-1".to_string(),
        name: "read_file".to_string(),
        parameters: HashMap::new(),
        result: Some(serde_json::Value::String("file contents".to_string())),
        result_summary: Some("Read 10 lines".to_string()),
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

    let json = serde_json::to_string(&tc).unwrap();
    let deserialized: ToolCall = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.nested_tool_calls.len(), 1);
    assert_eq!(deserialized.nested_tool_calls[0].name, "read_file");
}

#[test]
fn test_token_estimate() {
    let msg = ChatMessage {
        role: Role::Assistant,
        content: "a".repeat(400),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: vec![],
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    };
    assert_eq!(msg.token_estimate(), 100);

    // With explicit tokens set
    let msg2 = ChatMessage {
        tokens: Some(42),
        ..msg
    };
    assert_eq!(msg2.token_estimate(), 42);
}

#[test]
fn test_cache_token_estimate() {
    let mut msg = ChatMessage {
        role: Role::Assistant,
        content: "a".repeat(400),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: vec![],
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    };
    assert!(msg.tokens.is_none());
    let estimate = msg.cache_token_estimate();
    assert_eq!(estimate, 100);
    // Now tokens should be cached
    assert_eq!(msg.tokens, Some(100));
    // Subsequent calls return the cached value
    assert_eq!(msg.cache_token_estimate(), 100);
}

#[test]
fn test_provenance_serialization() {
    let prov = InputProvenance {
        kind: ProvenanceKind::ExternalUser,
        source_channel: Some("telegram".to_string()),
        source_session_id: None,
        timestamp: Utc::now(),
    };

    let json = serde_json::to_string(&prov).unwrap();
    assert!(json.contains("\"external_user\""));
    assert!(json.contains("\"telegram\""));
    // source_session_id should be skipped (None)
    assert!(!json.contains("source_session_id"));
}
