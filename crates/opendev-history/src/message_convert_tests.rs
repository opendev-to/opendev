use super::*;

fn make_user(content: &str) -> ChatMessage {
    ChatMessage {
        role: Role::User,
        content: content.to_string(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: Vec::new(),
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    }
}

fn make_assistant(content: &str) -> ChatMessage {
    ChatMessage {
        role: Role::Assistant,
        content: content.to_string(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: Vec::new(),
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    }
}

fn make_assistant_with_tools(content: &str, tool_calls: Vec<ToolCall>) -> ChatMessage {
    ChatMessage {
        role: Role::Assistant,
        content: content.to_string(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls,
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    }
}

fn make_tool_call(id: &str, name: &str, result: Option<&str>, error: Option<&str>) -> ToolCall {
    ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        parameters: HashMap::new(),
        result: result.map(|s| Value::String(s.to_string())),
        result_summary: None,
        timestamp: Utc::now(),
        approved: true,
        error: error.map(String::from),
        nested_tool_calls: Vec::new(),
    }
}

#[test]
fn test_simple_roundtrip() {
    let messages = vec![make_user("Hello"), make_assistant("Hi there!")];

    let api_values = chatmessages_to_api_values(&messages);
    assert_eq!(api_values.len(), 2);
    assert_eq!(api_values[0]["role"], "user");
    assert_eq!(api_values[1]["role"], "assistant");

    let restored = api_values_to_chatmessages(&api_values);
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[0].role, Role::User);
    assert_eq!(restored[0].content, "Hello");
    assert_eq!(restored[1].role, Role::Assistant);
    assert_eq!(restored[1].content, "Hi there!");
}

#[test]
fn test_tool_calls_roundtrip() {
    let tc = make_tool_call("tc-1", "bash", Some("output here"), None);
    let messages = vec![
        make_user("Run ls"),
        make_assistant_with_tools("Let me run that.", vec![tc]),
    ];

    let api_values = chatmessages_to_api_values(&messages);
    // user + assistant + tool result = 3
    assert_eq!(api_values.len(), 3);
    assert_eq!(api_values[1]["role"], "assistant");
    assert!(api_values[1]["tool_calls"].is_array());
    assert_eq!(api_values[2]["role"], "tool");
    assert_eq!(api_values[2]["tool_call_id"], "tc-1");

    let restored = api_values_to_chatmessages(&api_values);
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[1].tool_calls.len(), 1);
    assert_eq!(restored[1].tool_calls[0].name, "bash");
    assert!(restored[1].tool_calls[0].result.is_some());
}

#[test]
fn test_tool_call_error_roundtrip() {
    let tc = make_tool_call("tc-2", "bash", None, Some("command not found"));
    let messages = vec![make_assistant_with_tools("Running command", vec![tc])];

    let api_values = chatmessages_to_api_values(&messages);
    assert_eq!(api_values.len(), 2);
    assert_eq!(api_values[1]["content"], "Error: command not found");

    let restored = api_values_to_chatmessages(&api_values);
    assert_eq!(restored.len(), 1);
    assert!(restored[0].tool_calls[0].error.is_some());
    assert_eq!(
        restored[0].tool_calls[0].error.as_deref(),
        Some("command not found")
    );
}

#[test]
fn test_empty_content_with_tool_calls() {
    let tc = make_tool_call("tc-3", "read_file", Some("file contents"), None);
    let messages = vec![make_assistant_with_tools("", vec![tc])];

    let api_values = chatmessages_to_api_values(&messages);
    assert!(api_values[0]["content"].is_null());

    let restored = api_values_to_chatmessages(&api_values);
    assert_eq!(restored.len(), 1);
    assert!(restored[0].content.is_empty());
    assert_eq!(restored[0].tool_calls.len(), 1);
}

#[test]
fn test_thinking_trace_preserved() {
    let mut msg = make_assistant("Got it.");
    msg.thinking_trace = Some("I should check the file first.".to_string());
    let messages = vec![make_user("Do this"), msg];

    let api_values = chatmessages_to_api_values(&messages);
    assert_eq!(
        api_values[1]["_thinking_trace"],
        "I should check the file first."
    );

    let restored = api_values_to_chatmessages(&api_values);
    assert_eq!(
        restored[1].thinking_trace.as_deref(),
        Some("I should check the file first.")
    );
}

#[test]
fn test_incomplete_tool_call_gets_synthetic_error() {
    // Simulate an assistant message with tool_calls but no subsequent tool result
    let api_values = vec![
        json!({"role": "user", "content": "Run ls"}),
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "tc-orphan",
                "type": "function",
                "function": {
                    "name": "bash",
                    "arguments": "{}"
                }
            }]
        }),
        // No tool result message follows
    ];

    let restored = api_values_to_chatmessages(&api_values);
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[1].tool_calls.len(), 1);
    assert!(restored[1].tool_calls[0].error.is_some());
    assert!(
        restored[1].tool_calls[0]
            .error
            .as_deref()
            .unwrap()
            .contains("interrupted")
    );
}

#[test]
fn test_thinking_marker_skipped() {
    let api_values = vec![
        json!({"role": "user", "content": "Hello"}),
        json!({"role": "assistant", "content": "Hi"}),
        json!({"role": "user", "content": "Think about this", "_thinking": true}),
        json!({"role": "user", "content": "Next question"}),
    ];

    let restored = api_values_to_chatmessages(&api_values);
    // The _thinking user message should be skipped, leaving 3 messages
    assert_eq!(restored.len(), 3);
    assert_eq!(restored[0].content, "Hello");
    assert_eq!(restored[1].content, "Hi");
    assert_eq!(restored[2].content, "Next question");
}

#[test]
fn test_multiple_tool_calls() {
    let tc1 = make_tool_call("tc-a", "bash", Some("result 1"), None);
    let tc2 = make_tool_call("tc-b", "read_file", Some("result 2"), None);
    let messages = vec![make_assistant_with_tools(
        "Running multiple tools",
        vec![tc1, tc2],
    )];

    let api_values = chatmessages_to_api_values(&messages);
    // assistant + 2 tool results = 3
    assert_eq!(api_values.len(), 3);

    let restored = api_values_to_chatmessages(&api_values);
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].tool_calls.len(), 2);
}

#[test]
fn test_system_message_roundtrip() {
    let messages = vec![ChatMessage {
        role: Role::System,
        content: "You are a helpful assistant.".to_string(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: Vec::new(),
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    }];

    let api_values = chatmessages_to_api_values(&messages);
    assert_eq!(api_values[0]["role"], "system");

    let restored = api_values_to_chatmessages(&api_values);
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].role, Role::System);
}

#[test]
fn test_msg_class_preserved_roundtrip() {
    // System-injected nudge message should preserve _msg_class through roundtrip
    let api_values = vec![
        json!({"role": "user", "content": "[SYSTEM] Before finishing, verify...", "_msg_class": "nudge"}),
        json!({"role": "assistant", "content": "Done."}),
    ];

    let restored = api_values_to_chatmessages(&api_values);
    assert_eq!(restored.len(), 2);
    assert_eq!(
        restored[0]
            .metadata
            .get("_msg_class")
            .and_then(|v| v.as_str()),
        Some("nudge"),
        "_msg_class should be preserved in metadata"
    );

    // Convert back to API values — _msg_class should survive
    let re_api = chatmessages_to_api_values(&restored);
    assert_eq!(re_api[0]["_msg_class"], "nudge");
}

#[test]
fn test_msg_class_not_added_for_normal_messages() {
    let api_values = vec![json!({"role": "user", "content": "Hello"})];

    let restored = api_values_to_chatmessages(&api_values);
    assert!(
        !restored[0].metadata.contains_key("_msg_class"),
        "Normal user messages should not have _msg_class"
    );
}
