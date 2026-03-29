use super::*;
use crate::adapters::base::ProviderAdapter;

#[test]
fn test_provider_name() {
    let adapter = MistralAdapter::new();
    assert_eq!(adapter.provider_name(), "mistral");
}

#[test]
fn test_api_url_default() {
    let adapter = MistralAdapter::new();
    assert_eq!(adapter.api_url(), DEFAULT_API_URL);
}

#[test]
fn test_api_url_custom() {
    let adapter = MistralAdapter::with_url("https://my-proxy.com/v1/chat/completions");
    assert_eq!(
        adapter.api_url(),
        "https://my-proxy.com/v1/chat/completions"
    );
}

#[test]
fn test_convert_request_passthrough() {
    let adapter = MistralAdapter::new();
    let payload = json!({
        "model": "mistral-large-latest",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"}
        ],
        "temperature": 0.7,
        "max_tokens": 1024
    });
    let result = adapter.convert_request(payload.clone());

    // Core fields should be preserved
    assert_eq!(result["model"], "mistral-large-latest");
    assert_eq!(result["temperature"], 0.7);
    assert_eq!(result["max_tokens"], 1024);
    assert_eq!(result["messages"].as_array().unwrap().len(), 2);
}

#[test]
fn test_convert_request_removes_unsupported() {
    let adapter = MistralAdapter::new();
    let payload = json!({
        "model": "mistral-large-latest",
        "messages": [{"role": "user", "content": "Hi"}],
        "logprobs": true,
        "top_logprobs": 5,
        "n": 2,
        "seed": 42
    });
    let result = adapter.convert_request(payload);

    assert!(result.get("logprobs").is_none());
    assert!(result.get("top_logprobs").is_none());
    assert!(result.get("n").is_none());
    assert!(result.get("seed").is_none());
    // Model and messages preserved
    assert_eq!(result["model"], "mistral-large-latest");
}

#[test]
fn test_convert_response_passthrough() {
    let adapter = MistralAdapter::new();
    let response = json!({
        "id": "cmpl-123",
        "object": "chat.completion",
        "model": "mistral-large-latest",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello!"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15
        }
    });
    let result = adapter.convert_response(response);

    assert_eq!(result["choices"][0]["message"]["content"], "Hello!");
    assert_eq!(result["choices"][0]["finish_reason"], "stop");
}

#[test]
fn test_normalize_tool_calls_missing_type() {
    let adapter = MistralAdapter::new();
    let response = json!({
        "id": "cmpl-456",
        "object": "chat.completion",
        "model": "mistral-large-latest",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_abc",
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
    let result = adapter.convert_response(response);

    let tc = &result["choices"][0]["message"]["tool_calls"][0];
    assert_eq!(tc["type"], "function");
    assert_eq!(tc["id"], "call_abc");
    assert_eq!(tc["function"]["name"], "read_file");
}

#[test]
fn test_normalize_tool_calls_object_arguments() {
    let adapter = MistralAdapter::new();
    let response = json!({
        "id": "cmpl-789",
        "object": "chat.completion",
        "model": "mistral-large-latest",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_xyz",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": {"path": "test.txt"}
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
    });
    let result = adapter.convert_response(response);

    let args = result["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .unwrap();
    // Should be serialized to a JSON string
    let parsed: Value = serde_json::from_str(args).unwrap();
    assert_eq!(parsed["path"], "test.txt");
}

#[test]
fn test_extra_headers_empty() {
    let adapter = MistralAdapter::new();
    assert!(adapter.extra_headers().is_empty());
}
