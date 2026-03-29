use super::*;
use crate::adapters::base::ProviderAdapter;

#[test]
fn test_provider_name() {
    let adapter = OpenAiAdapter::new();
    assert_eq!(adapter.provider_name(), "openai");
}

#[test]
fn test_api_url_default() {
    let adapter = OpenAiAdapter::new();
    assert_eq!(adapter.api_url(), "https://api.openai.com/v1/responses");
}

#[test]
fn test_api_url_custom() {
    let adapter = OpenAiAdapter::with_url("https://my-proxy.com/v1/responses");
    assert_eq!(adapter.api_url(), "https://my-proxy.com/v1/responses");
}

#[test]
fn test_is_reasoning_model() {
    assert!(OpenAiAdapter::is_reasoning_model(
        &json!({"model": "o1-preview"})
    ));
    assert!(OpenAiAdapter::is_reasoning_model(
        &json!({"model": "o1-mini"})
    ));
    assert!(OpenAiAdapter::is_reasoning_model(
        &json!({"model": "o3-mini"})
    ));
    assert!(!OpenAiAdapter::is_reasoning_model(
        &json!({"model": "gpt-4"})
    ));
    assert!(!OpenAiAdapter::is_reasoning_model(
        &json!({"model": "claude-3"})
    ));
}

#[test]
fn test_convert_request_basic() {
    let adapter = OpenAiAdapter::new();
    let payload = json!({
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"}
        ],
        "temperature": 0.7,
        "max_tokens": 1024
    });
    let result = adapter.convert_request(payload);

    // Should have instructions from system message
    assert_eq!(result["instructions"], "You are helpful.");
    // Should have input items
    let input = result["input"].as_array().unwrap();
    assert_eq!(input.len(), 1);
    assert_eq!(input[0]["type"], "message");
    assert_eq!(input[0]["role"], "user");
    assert_eq!(input[0]["content"], "Hello");
    // store: false
    assert_eq!(result["store"], false);
    // max_output_tokens
    assert_eq!(result["max_output_tokens"], 1024);
    // temperature preserved for non-reasoning models
    assert_eq!(result["temperature"], 0.7);
    // No messages key in output
    assert!(result.get("messages").is_none());
}

#[test]
fn test_convert_request_reasoning_model_strips_temperature() {
    let adapter = OpenAiAdapter::new();
    let payload = json!({
        "model": "o1-preview",
        "messages": [
            {"role": "user", "content": "Think about this"}
        ],
        "temperature": 0.7
    });
    let result = adapter.convert_request(payload);

    // Temperature should be stripped for reasoning models
    assert!(result.get("temperature").is_none());
}

#[test]
fn test_convert_request_with_tools() {
    let adapter = OpenAiAdapter::new();
    let payload = json!({
        "model": "gpt-4o",
        "messages": [
            {"role": "user", "content": "Read file"},
            {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_123",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\": \"test.txt\"}"
                    }
                }]
            },
            {
                "role": "tool",
                "tool_call_id": "call_123",
                "content": "file contents here"
            }
        ],
        "tools": [{
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file",
                "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
            }
        }]
    });
    let result = adapter.convert_request(payload);

    let input = result["input"].as_array().unwrap();
    // user message + function_call + function_call_output = 3 items
    assert_eq!(input.len(), 3);

    // Check function_call
    assert_eq!(input[1]["type"], "function_call");
    assert_eq!(input[1]["call_id"], "call_123");
    assert_eq!(input[1]["name"], "read_file");
    assert_eq!(input[1]["arguments"], "{\"path\": \"test.txt\"}");

    // Check function_call_output
    assert_eq!(input[2]["type"], "function_call_output");
    assert_eq!(input[2]["call_id"], "call_123");
    assert_eq!(input[2]["output"], "file contents here");

    // Check tools flattened
    let tools = result["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["type"], "function");
    assert_eq!(tools[0]["name"], "read_file");
    assert_eq!(tools[0]["description"], "Read a file");
    assert!(tools[0].get("function").is_none()); // flattened, no nested function
}

#[test]
fn test_convert_response_text() {
    let adapter = OpenAiAdapter::new();
    let response = json!({
        "id": "resp_123",
        "model": "gpt-4o",
        "status": "completed",
        "output": [{
            "type": "message",
            "content": [
                {"type": "output_text", "text": "Hello! How can I help?"}
            ]
        }],
        "usage": {
            "input_tokens": 10,
            "output_tokens": 8
        }
    });
    let result = adapter.convert_response(response);

    assert_eq!(result["object"], "chat.completion");
    assert_eq!(result["id"], "resp_123");
    assert_eq!(result["model"], "gpt-4o");

    let choice = &result["choices"][0];
    assert_eq!(choice["finish_reason"], "stop");
    assert_eq!(choice["message"]["role"], "assistant");
    assert_eq!(choice["message"]["content"], "Hello! How can I help?");

    assert_eq!(result["usage"]["prompt_tokens"], 10);
    assert_eq!(result["usage"]["completion_tokens"], 8);
    assert_eq!(result["usage"]["total_tokens"], 18);
}

#[test]
fn test_convert_response_tool_calls() {
    let adapter = OpenAiAdapter::new();
    let response = json!({
        "id": "resp_456",
        "model": "gpt-4o",
        "status": "completed",
        "output": [{
            "type": "function_call",
            "call_id": "call_abc",
            "name": "read_file",
            "arguments": "{\"path\": \"test.txt\"}"
        }],
        "usage": {"input_tokens": 5, "output_tokens": 3}
    });
    let result = adapter.convert_response(response);

    let choice = &result["choices"][0];
    assert_eq!(choice["finish_reason"], "tool_calls");

    let tool_calls = choice["message"]["tool_calls"].as_array().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["id"], "call_abc");
    assert_eq!(tool_calls[0]["type"], "function");
    assert_eq!(tool_calls[0]["function"]["name"], "read_file");
    assert_eq!(
        tool_calls[0]["function"]["arguments"],
        "{\"path\": \"test.txt\"}"
    );
}

#[test]
fn test_convert_response_reasoning() {
    let adapter = OpenAiAdapter::new();
    let response = json!({
        "id": "resp_789",
        "model": "o1-preview",
        "status": "completed",
        "output": [
            {
                "type": "reasoning",
                "summary": [{"text": "Let me think about this..."}]
            },
            {
                "type": "message",
                "content": [{"type": "output_text", "text": "The answer is 42."}]
            }
        ],
        "usage": {"input_tokens": 10, "output_tokens": 20}
    });
    let result = adapter.convert_response(response);

    let message = &result["choices"][0]["message"];
    assert_eq!(message["content"], "The answer is 42.");
    assert_eq!(message["reasoning_content"], "Let me think about this...");
}

#[test]
fn test_convert_response_incomplete() {
    let adapter = OpenAiAdapter::new();
    let response = json!({
        "id": "resp_inc",
        "model": "gpt-4o",
        "status": "incomplete",
        "output": [{
            "type": "message",
            "content": [{"type": "output_text", "text": "partial..."}]
        }],
        "usage": {"input_tokens": 5, "output_tokens": 50}
    });
    let result = adapter.convert_response(response);
    assert_eq!(result["choices"][0]["finish_reason"], "length");
}

#[test]
fn test_convert_content_blocks_with_image() {
    let adapter = OpenAiAdapter::new();
    let payload = json!({
        "model": "gpt-4o",
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "What is this?"},
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/jpeg",
                        "data": "base64data"
                    }
                }
            ]
        }]
    });
    let result = adapter.convert_request(payload);
    let content = result["input"][0]["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "input_text");
    assert_eq!(content[0]["text"], "What is this?");
    assert_eq!(content[1]["type"], "input_image");
    assert_eq!(content[1]["image_url"], "data:image/jpeg;base64,base64data");
}

#[test]
fn test_extra_headers_empty() {
    let adapter = OpenAiAdapter::new();
    assert!(adapter.extra_headers().is_empty());
}
