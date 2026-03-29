use super::*;
use crate::adapters::base::ProviderAdapter;

#[test]
fn test_provider_name() {
    let adapter = GeminiAdapter::new("gemini-2.0-flash");
    assert_eq!(adapter.provider_name(), "gemini");
}

#[test]
fn test_api_url() {
    let adapter = GeminiAdapter::new("gemini-2.0-flash");
    assert_eq!(adapter.api_url(), DEFAULT_BASE_URL);
}

#[test]
fn test_gemini_api_url_builder() {
    let url = gemini_api_url(DEFAULT_BASE_URL, "gemini-2.0-flash");
    assert_eq!(
        url,
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent"
    );
}

#[test]
fn test_convert_request_basic() {
    let adapter = GeminiAdapter::new("gemini-2.0-flash");
    let payload = json!({
        "model": "gemini-2.0-flash",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"}
        ],
        "temperature": 0.7,
        "max_tokens": 1024
    });
    let result = adapter.convert_request(payload);

    // System instruction extracted
    assert_eq!(
        result["systemInstruction"]["parts"][0]["text"],
        "You are helpful."
    );

    // Contents should have only the user message
    let contents = result["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0]["role"], "user");
    assert_eq!(contents[0]["parts"][0]["text"], "Hello");

    // Generation config
    assert_eq!(result["generationConfig"]["temperature"], 0.7);
    assert_eq!(result["generationConfig"]["maxOutputTokens"], 1024);
}

#[test]
fn test_convert_request_with_tools() {
    let adapter = GeminiAdapter::new("gemini-2.0-flash");
    let payload = json!({
        "model": "gemini-2.0-flash",
        "messages": [
            {"role": "user", "content": "Read a file"}
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

    let tools = result["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    let decls = tools[0]["functionDeclarations"].as_array().unwrap();
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0]["name"], "read_file");
    assert_eq!(decls[0]["description"], "Read a file");
}

#[test]
fn test_convert_request_with_tool_calls() {
    let adapter = GeminiAdapter::new("gemini-2.0-flash");
    let payload = json!({
        "model": "gemini-2.0-flash",
        "messages": [
            {"role": "user", "content": "Read test.txt"},
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
                "name": "read_file",
                "tool_call_id": "call_123",
                "content": "file contents"
            }
        ]
    });
    let result = adapter.convert_request(payload);

    let contents = result["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 3);

    // User message
    assert_eq!(contents[0]["role"], "user");

    // Assistant with functionCall
    assert_eq!(contents[1]["role"], "model");
    assert!(contents[1]["parts"][0].get("functionCall").is_some());
    assert_eq!(contents[1]["parts"][0]["functionCall"]["name"], "read_file");

    // Tool result as functionResponse
    assert_eq!(contents[2]["role"], "user");
    assert!(contents[2]["parts"][0].get("functionResponse").is_some());
    assert_eq!(
        contents[2]["parts"][0]["functionResponse"]["name"],
        "read_file"
    );
}

#[test]
fn test_convert_response_text() {
    let adapter = GeminiAdapter::new("gemini-2.0-flash");
    let response = json!({
        "candidates": [{
            "content": {
                "parts": [{"text": "Hello! How can I help?"}],
                "role": "model"
            },
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 10,
            "candidatesTokenCount": 8
        }
    });
    let result = adapter.convert_response(response);

    assert_eq!(result["object"], "chat.completion");
    assert_eq!(
        result["choices"][0]["message"]["content"],
        "Hello! How can I help?"
    );
    assert_eq!(result["choices"][0]["finish_reason"], "stop");
    assert_eq!(result["usage"]["prompt_tokens"], 10);
    assert_eq!(result["usage"]["completion_tokens"], 8);
    assert_eq!(result["usage"]["total_tokens"], 18);
}

#[test]
fn test_convert_response_function_call() {
    let adapter = GeminiAdapter::new("gemini-2.0-flash");
    let response = json!({
        "candidates": [{
            "content": {
                "parts": [{
                    "functionCall": {
                        "name": "read_file",
                        "args": {"path": "test.txt"}
                    }
                }],
                "role": "model"
            },
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 5,
            "candidatesTokenCount": 3
        }
    });
    let result = adapter.convert_response(response);

    assert_eq!(result["choices"][0]["finish_reason"], "tool_calls");
    let tool_calls = result["choices"][0]["message"]["tool_calls"]
        .as_array()
        .unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["function"]["name"], "read_file");
}

#[test]
fn test_convert_response_max_tokens() {
    let adapter = GeminiAdapter::new("gemini-2.0-flash");
    let response = json!({
        "candidates": [{
            "content": {
                "parts": [{"text": "partial..."}],
                "role": "model"
            },
            "finishReason": "MAX_TOKENS"
        }],
        "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 50}
    });
    let result = adapter.convert_response(response);
    assert_eq!(result["choices"][0]["finish_reason"], "length");
}

#[test]
fn test_convert_response_safety() {
    let adapter = GeminiAdapter::new("gemini-2.0-flash");
    let response = json!({
        "candidates": [{
            "content": {
                "parts": [{"text": ""}],
                "role": "model"
            },
            "finishReason": "SAFETY"
        }],
        "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 0}
    });
    let result = adapter.convert_response(response);
    assert_eq!(result["choices"][0]["finish_reason"], "content_filter");
}

#[test]
fn test_default_model() {
    let adapter = GeminiAdapter::default();
    assert_eq!(adapter.model, "gemini-2.0-flash");
}

#[test]
fn test_custom_base_url() {
    let adapter = GeminiAdapter::new("gemini-pro").with_base_url("https://my-proxy.com/v1");
    assert_eq!(adapter.api_url(), "https://my-proxy.com/v1");
    let url = gemini_api_url(adapter.api_url(), "gemini-pro");
    assert_eq!(
        url,
        "https://my-proxy.com/v1/models/gemini-pro:generateContent"
    );
}
