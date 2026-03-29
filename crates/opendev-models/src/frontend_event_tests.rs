use super::*;

#[test]
fn test_frontend_event_serialization() {
    let event = FrontendEvent::MessageChunk(MessageChunkPayload {
        session_id: "test".to_string(),
        content: "hello".to_string(),
    });

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"MessageChunk\""));
    assert!(json.contains("\"content\":\"hello\""));
}

#[test]
fn test_status_update_optional_fields() {
    let payload = StatusUpdatePayload {
        session_id: "s1".to_string(),
        model: None,
        provider: None,
        input_tokens: Some(100),
        output_tokens: Some(50),
        context_usage_pct: None,
        session_cost_usd: None,
        git_branch: None,
        autonomy_level: None,
        thinking_level: None,
        file_changes: None,
        todos: None,
    };

    let json = serde_json::to_string(&payload).unwrap();
    assert!(json.contains("\"input_tokens\":100"));
    assert!(!json.contains("\"model\""));
}
