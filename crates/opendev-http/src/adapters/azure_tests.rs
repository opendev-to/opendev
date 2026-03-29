use super::*;
use crate::adapters::base::ProviderAdapter;

#[test]
fn test_provider_name() {
    let adapter = AzureOpenAiAdapter::new("https://myresource.openai.azure.com", "gpt-4o");
    assert_eq!(adapter.provider_name(), "azure");
}

#[test]
fn test_api_url_default_version() {
    let adapter = AzureOpenAiAdapter::new("https://myresource.openai.azure.com", "gpt-4o");
    assert_eq!(
        adapter.api_url(),
        "https://myresource.openai.azure.com/openai/deployments/gpt-4o/chat/completions?api-version=2024-02-15-preview"
    );
}

#[test]
fn test_api_url_custom_version() {
    let adapter = AzureOpenAiAdapter::new("https://myresource.openai.azure.com", "gpt-4o")
        .with_api_version("2024-06-01");
    assert_eq!(
        adapter.api_url(),
        "https://myresource.openai.azure.com/openai/deployments/gpt-4o/chat/completions?api-version=2024-06-01"
    );
}

#[test]
fn test_api_url_trailing_slash() {
    let adapter = AzureOpenAiAdapter::new("https://myresource.openai.azure.com/", "gpt-4o");
    assert_eq!(
        adapter.api_url(),
        "https://myresource.openai.azure.com/openai/deployments/gpt-4o/chat/completions?api-version=2024-02-15-preview"
    );
}

#[test]
fn test_convert_request_strips_model() {
    let adapter = AzureOpenAiAdapter::new("https://myresource.openai.azure.com", "gpt-4o");
    let payload = serde_json::json!({
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"}
        ],
        "temperature": 0.7,
        "max_tokens": 1024
    });
    let result = adapter.convert_request(payload);

    // model should be stripped (it's in the URL)
    assert!(result.get("model").is_none());
    // Other fields preserved
    assert_eq!(result["temperature"], 0.7);
    assert_eq!(result["max_tokens"], 1024);
    assert_eq!(result["messages"].as_array().unwrap().len(), 2);
}

#[test]
fn test_convert_response_passthrough() {
    let adapter = AzureOpenAiAdapter::new("https://myresource.openai.azure.com", "gpt-4o");
    let response = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "model": "gpt-4o",
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
fn test_convert_request_with_tools() {
    let adapter = AzureOpenAiAdapter::new("https://myresource.openai.azure.com", "gpt-4o");
    let payload = serde_json::json!({
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": "Read a file"}],
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

    // model stripped, tools preserved
    assert!(result.get("model").is_none());
    assert_eq!(result["tools"].as_array().unwrap().len(), 1);
}

#[test]
fn test_build_azure_url() {
    let url = build_azure_url(
        "https://myresource.openai.azure.com",
        "gpt-4o",
        "2024-02-15-preview",
    );
    assert_eq!(
        url,
        "https://myresource.openai.azure.com/openai/deployments/gpt-4o/chat/completions?api-version=2024-02-15-preview"
    );
}

#[test]
fn test_extra_headers() {
    let adapter = AzureOpenAiAdapter::new("https://myresource.openai.azure.com", "gpt-4o");
    // Azure adapter doesn't add extra headers via the trait
    // (api-key is handled by the HTTP client layer)
    let headers = adapter.extra_headers();
    assert!(headers.is_empty());
}
