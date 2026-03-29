use super::*;
use crate::adapters::base::ProviderAdapter;

#[test]
fn test_provider_name() {
    let adapter = OllamaAdapter::new();
    assert_eq!(adapter.provider_name(), "ollama");
}

#[test]
fn test_api_url_default() {
    let adapter = OllamaAdapter::new();
    assert_eq!(adapter.api_url(), DEFAULT_API_URL);
}

#[test]
fn test_api_url_custom() {
    let adapter = OllamaAdapter::with_url("http://remote-host:11434/v1/chat/completions");
    assert_eq!(
        adapter.api_url(),
        "http://remote-host:11434/v1/chat/completions"
    );
}

#[test]
fn test_convert_request_passthrough() {
    let adapter = OllamaAdapter::new();
    let payload = serde_json::json!({
        "model": "llama3:8b",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"}
        ],
        "temperature": 0.7,
        "max_tokens": 1024
    });
    let result = adapter.convert_request(payload);

    assert_eq!(result["model"], "llama3:8b");
    assert_eq!(result["temperature"], 0.7);
    assert_eq!(result["max_tokens"], 1024);
    assert_eq!(result["messages"].as_array().unwrap().len(), 2);
}

#[test]
fn test_convert_request_removes_unsupported() {
    let adapter = OllamaAdapter::new();
    let payload = serde_json::json!({
        "model": "llama3:8b",
        "messages": [{"role": "user", "content": "Hi"}],
        "logprobs": true,
        "top_logprobs": 5,
        "n": 2,
        "frequency_penalty": 0.5,
        "presence_penalty": 0.5,
        "seed": 42
    });
    let result = adapter.convert_request(payload);

    assert!(result.get("logprobs").is_none());
    assert!(result.get("top_logprobs").is_none());
    assert!(result.get("n").is_none());
    assert!(result.get("frequency_penalty").is_none());
    assert!(result.get("presence_penalty").is_none());
    assert!(result.get("seed").is_none());
    assert_eq!(result["model"], "llama3:8b");
}

#[test]
fn test_convert_request_max_completion_tokens() {
    let adapter = OllamaAdapter::new();
    let payload = serde_json::json!({
        "model": "llama3:8b",
        "messages": [{"role": "user", "content": "Hi"}],
        "max_completion_tokens": 2048
    });
    let result = adapter.convert_request(payload);

    // max_completion_tokens should be converted to max_tokens
    assert!(result.get("max_completion_tokens").is_none());
    assert_eq!(result["max_tokens"], 2048);
}

#[test]
fn test_convert_request_max_completion_tokens_no_override() {
    let adapter = OllamaAdapter::new();
    let payload = serde_json::json!({
        "model": "llama3:8b",
        "messages": [{"role": "user", "content": "Hi"}],
        "max_tokens": 1024,
        "max_completion_tokens": 2048
    });
    let result = adapter.convert_request(payload);

    // Existing max_tokens should not be overridden
    assert!(result.get("max_completion_tokens").is_none());
    assert_eq!(result["max_tokens"], 1024);
}

#[test]
fn test_convert_response_passthrough() {
    let adapter = OllamaAdapter::new();
    let response = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "model": "llama3:8b",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help?"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 8,
            "total_tokens": 18
        }
    });
    let result = adapter.convert_response(response.clone());
    assert_eq!(result, response);
}

#[test]
fn test_extra_headers_empty() {
    let adapter = OllamaAdapter::new();
    assert!(adapter.extra_headers().is_empty());
}

#[test]
fn test_convert_response_with_tool_calls() {
    let adapter = OllamaAdapter::new();
    let response = serde_json::json!({
        "id": "chatcmpl-456",
        "object": "chat.completion",
        "model": "llama3:8b",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_abc",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\": \"test.txt\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
    });
    let result = adapter.convert_response(response.clone());
    assert_eq!(result, response);
}
