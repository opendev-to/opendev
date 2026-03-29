use super::*;

#[test]
fn test_safe_json_loads_plain() {
    let result = safe_json_loads(r#"{"key": "value"}"#);
    assert!(result.is_ok());
    assert_eq!(result.unwrap()["key"], "value");
}

#[test]
fn test_safe_json_loads_with_code_fence() {
    let result = safe_json_loads("```json\n{\"key\": \"value\"}\n```");
    assert!(result.is_ok());
    assert_eq!(result.unwrap()["key"], "value");
}

#[test]
fn test_safe_json_loads_bare_fence() {
    let result = safe_json_loads("```\n{\"key\": \"value\"}\n```");
    assert!(result.is_ok());
}

#[test]
fn test_safe_json_loads_not_object() {
    let result = safe_json_loads("[1, 2, 3]");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Expected a JSON object"));
}

#[test]
fn test_safe_json_loads_invalid() {
    let result = safe_json_loads("not json at all");
    assert!(result.is_err());
}

#[test]
fn test_safe_json_loads_truncated() {
    let result = safe_json_loads(r#"{"key": "value", "other": {"#);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("truncated"));
}

#[test]
fn test_agent_response_new() {
    let resp = AgentResponse::new("Hello world");
    assert_eq!(resp.content, "Hello world");
    assert!(resp.reasoning.is_none());
    assert!(resp.tool_calls.is_empty());
}

#[test]
fn test_reflector_output_from_json() {
    let data = serde_json::json!({
        "reasoning": "The approach was correct",
        "error_identification": "No errors",
        "root_cause_analysis": "N/A",
        "correct_approach": "Search then read",
        "key_insight": "Always search first",
        "bullet_tags": [
            {"id": "t-001", "tag": "HELPFUL"},
            {"id": "t-002", "tag": "neutral"}
        ]
    });

    let output = ReflectorOutput::from_json(&data);
    assert_eq!(output.reasoning, "The approach was correct");
    assert_eq!(output.key_insight, "Always search first");
    assert_eq!(output.bullet_tags.len(), 2);
    assert_eq!(output.bullet_tags[0].id, "t-001");
    assert_eq!(output.bullet_tags[0].tag, "helpful"); // lowercased
    assert_eq!(output.bullet_tags[1].tag, "neutral");
}

#[test]
fn test_reflector_output_missing_fields() {
    let data = serde_json::json!({});
    let output = ReflectorOutput::from_json(&data);
    assert!(output.reasoning.is_empty());
    assert!(output.bullet_tags.is_empty());
}

#[test]
fn test_curator_output_from_json() {
    let data = serde_json::json!({
        "reasoning": "Adding a new strategy",
        "operations": [
            {
                "type": "ADD",
                "section": "testing",
                "content": "Always run tests"
            }
        ]
    });

    let output = CuratorOutput::from_json(&data);
    assert_eq!(output.delta.reasoning, "Adding a new strategy");
    assert_eq!(output.delta.operations.len(), 1);
}

#[test]
fn test_bullet_tag_serialization() {
    let tag = BulletTag {
        id: "t-001".to_string(),
        tag: "helpful".to_string(),
    };
    let json = serde_json::to_string(&tag).unwrap();
    let deserialized: BulletTag = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "t-001");
    assert_eq!(deserialized.tag, "helpful");
}

#[test]
fn test_agent_response_roundtrip() {
    let resp = AgentResponse {
        content: "Test response".to_string(),
        reasoning: Some("I thought about it".to_string()),
        tool_calls: vec![serde_json::json!({"function": {"name": "read_file"}})],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: AgentResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.content, "Test response");
    assert_eq!(
        deserialized.reasoning.as_deref(),
        Some("I thought about it")
    );
    assert_eq!(deserialized.tool_calls.len(), 1);
}
