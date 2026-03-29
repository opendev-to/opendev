use super::*;
use serde_json::json;

#[test]
fn test_response_to_chat_completions_text() {
    let response = json!({
        "id": "msg_123",
        "content": [{"type": "text", "text": "Hello!"}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 5}
    });
    let result = response_to_chat_completions(response, "anthropic.claude-3-sonnet");
    assert_eq!(result["object"], "chat.completion");
    assert_eq!(result["model"], "anthropic.claude-3-sonnet");
    assert_eq!(result["choices"][0]["message"]["content"], "Hello!");
    assert_eq!(result["choices"][0]["finish_reason"], "stop");
    assert_eq!(result["usage"]["prompt_tokens"], 10);
    assert_eq!(result["usage"]["completion_tokens"], 5);
    assert_eq!(result["usage"]["total_tokens"], 15);
}

#[test]
fn test_response_to_chat_completions_tool_use() {
    let response = json!({
        "id": "msg_456",
        "content": [
            {"type": "text", "text": "Let me read that file."},
            {
                "type": "tool_use",
                "id": "tu_1",
                "name": "read_file",
                "input": {"path": "src/main.rs"}
            }
        ],
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 20, "output_tokens": 10}
    });
    let result = response_to_chat_completions(response, "claude-3");
    assert_eq!(result["choices"][0]["finish_reason"], "tool_calls");
    let tool_calls = result["choices"][0]["message"]["tool_calls"]
        .as_array()
        .unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["id"], "tu_1");
    assert_eq!(tool_calls[0]["function"]["name"], "read_file");
}

#[test]
fn test_response_max_tokens_finish_reason() {
    let response = json!({
        "content": [{"type": "text", "text": "truncated..."}],
        "stop_reason": "max_tokens",
        "usage": {"input_tokens": 0, "output_tokens": 0}
    });
    let result = response_to_chat_completions(response, "model");
    assert_eq!(result["choices"][0]["finish_reason"], "length");
}
