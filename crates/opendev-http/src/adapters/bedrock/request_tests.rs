use super::*;
use serde_json::json;

#[test]
fn test_extract_system() {
    let mut payload = json!({
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"}
        ]
    });
    extract_system(&mut payload);
    assert_eq!(payload["system"], "You are helpful.");
    let messages = payload["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
}

#[test]
fn test_extract_system_multiple() {
    let mut payload = json!({
        "messages": [
            {"role": "system", "content": "Part 1"},
            {"role": "system", "content": "Part 2"},
            {"role": "user", "content": "Hello"}
        ]
    });
    extract_system(&mut payload);
    assert_eq!(payload["system"], "Part 1\n\nPart 2");
}

#[test]
fn test_extract_system_none() {
    let mut payload = json!({
        "messages": [
            {"role": "user", "content": "Hello"}
        ]
    });
    extract_system(&mut payload);
    assert!(payload.get("system").is_none());
}

#[test]
fn test_convert_tools() {
    let mut payload = json!({
        "tools": [{
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file",
                "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
            }
        }]
    });
    convert_tools(&mut payload);
    let tools = payload["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "read_file");
    assert_eq!(tools[0]["description"], "Read a file");
    assert!(tools[0].get("input_schema").is_some());
}

#[test]
fn test_convert_tool_messages() {
    let mut payload = json!({
        "messages": [
            {"role": "user", "content": "Read the file"},
            {
                "role": "assistant",
                "content": "I'll read it.",
                "tool_calls": [{
                    "id": "tc-1",
                    "function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}
                }]
            },
            {
                "role": "tool",
                "tool_call_id": "tc-1",
                "content": "fn main() {}"
            }
        ]
    });
    convert_tool_messages(&mut payload);
    let messages = payload["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);

    // Assistant message should have tool_use blocks
    let assistant = &messages[1];
    let content = assistant["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "tool_use");
    assert_eq!(content[1]["name"], "read_file");

    // Tool result should be converted to user with tool_result
    let tool_result = &messages[2];
    assert_eq!(tool_result["role"], "user");
    let blocks = tool_result["content"].as_array().unwrap();
    assert_eq!(blocks[0]["type"], "tool_result");
    assert_eq!(blocks[0]["tool_use_id"], "tc-1");
}

#[test]
fn test_convert_tool_messages_merge_consecutive() {
    let mut payload = json!({
        "messages": [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"id": "tc-1", "function": {"name": "read_file", "arguments": "{}"}},
                    {"id": "tc-2", "function": {"name": "search", "arguments": "{}"}}
                ]
            },
            {"role": "tool", "tool_call_id": "tc-1", "content": "file1"},
            {"role": "tool", "tool_call_id": "tc-2", "content": "file2"}
        ]
    });
    convert_tool_messages(&mut payload);
    let messages = payload["messages"].as_array().unwrap();
    // Two consecutive tool messages should be merged into one user message
    assert_eq!(messages.len(), 2);
    let user_msg = &messages[1];
    assert_eq!(user_msg["role"], "user");
    let blocks = user_msg["content"].as_array().unwrap();
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0]["tool_use_id"], "tc-1");
    assert_eq!(blocks[1]["tool_use_id"], "tc-2");
}

#[test]
fn test_ensure_max_tokens_default() {
    let mut payload = json!({"messages": []});
    ensure_max_tokens(&mut payload);
    assert_eq!(payload["max_tokens"], 4096);
}

#[test]
fn test_ensure_max_tokens_preserves_existing() {
    let mut payload = json!({"messages": [], "max_tokens": 8192});
    ensure_max_tokens(&mut payload);
    assert_eq!(payload["max_tokens"], 8192);
}

#[test]
fn test_ensure_max_tokens_converts_max_completion_tokens() {
    let mut payload = json!({"messages": [], "max_completion_tokens": 2048});
    ensure_max_tokens(&mut payload);
    assert_eq!(payload["max_tokens"], 2048);
    assert!(payload.get("max_completion_tokens").is_none());
}
