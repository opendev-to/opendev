use super::*;
use crate::adapters::base::ProviderAdapter;

#[test]
fn test_provider_name() {
    let adapter = GroqAdapter::new();
    assert_eq!(adapter.provider_name(), "groq");
}

#[test]
fn test_api_url_default() {
    let adapter = GroqAdapter::new();
    assert_eq!(adapter.api_url(), DEFAULT_API_URL);
}

#[test]
fn test_api_url_custom() {
    let adapter = GroqAdapter::with_url("https://my-proxy.com/v1/chat/completions");
    assert_eq!(
        adapter.api_url(),
        "https://my-proxy.com/v1/chat/completions"
    );
}

#[test]
fn test_convert_request_passthrough() {
    let adapter = GroqAdapter::new();
    let payload = json!({
        "model": "llama3-70b-8192",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"}
        ],
        "temperature": 0.7,
        "max_tokens": 1024
    });
    let result = adapter.convert_request(payload);

    assert_eq!(result["model"], "llama3-70b-8192");
    assert_eq!(result["temperature"], 0.7);
    assert_eq!(result["max_tokens"], 1024);
    assert_eq!(result["messages"].as_array().unwrap().len(), 2);
}

#[test]
fn test_convert_request_removes_unsupported() {
    let adapter = GroqAdapter::new();
    let payload = json!({
        "model": "llama3-70b-8192",
        "messages": [{"role": "user", "content": "Hi"}],
        "logprobs": true,
        "top_logprobs": 5,
        "n": 2
    });
    let result = adapter.convert_request(payload);

    assert!(result.get("logprobs").is_none());
    assert!(result.get("top_logprobs").is_none());
    assert!(result.get("n").is_none());
    assert_eq!(result["model"], "llama3-70b-8192");
}

#[test]
fn test_convert_response_passthrough() {
    let adapter = GroqAdapter::new();
    let response = json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "model": "llama3-70b-8192",
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
fn test_rate_limit_info_from_headers() {
    let headers = vec![
        ("x-ratelimit-limit-requests".to_string(), "30".to_string()),
        ("x-ratelimit-limit-tokens".to_string(), "30000".to_string()),
        (
            "x-ratelimit-remaining-requests".to_string(),
            "29".to_string(),
        ),
        (
            "x-ratelimit-remaining-tokens".to_string(),
            "29500".to_string(),
        ),
        ("x-ratelimit-reset-requests".to_string(), "2s".to_string()),
        ("x-ratelimit-reset-tokens".to_string(), "1s".to_string()),
    ];
    let info = RateLimitInfo::from_headers(&headers);

    assert_eq!(info.limit_requests, Some(30));
    assert_eq!(info.limit_tokens, Some(30000));
    assert_eq!(info.remaining_requests, Some(29));
    assert_eq!(info.remaining_tokens, Some(29500));
    assert_eq!(info.reset_requests, Some("2s".to_string()));
    assert_eq!(info.reset_tokens, Some("1s".to_string()));
}

#[test]
fn test_rate_limit_info_partial_headers() {
    let headers = vec![(
        "x-ratelimit-remaining-requests".to_string(),
        "5".to_string(),
    )];
    let info = RateLimitInfo::from_headers(&headers);

    assert_eq!(info.limit_requests, None);
    assert_eq!(info.remaining_requests, Some(5));
    assert_eq!(info.remaining_tokens, None);
}

#[test]
fn test_rate_limit_info_to_json() {
    let info = RateLimitInfo {
        limit_requests: Some(30),
        limit_tokens: Some(30000),
        remaining_requests: Some(29),
        remaining_tokens: None,
        reset_requests: Some("2s".to_string()),
        reset_tokens: None,
    };
    let j = info.to_json();
    assert_eq!(j["limit_requests"], 30);
    assert_eq!(j["limit_tokens"], 30000);
    assert_eq!(j["remaining_requests"], 29);
    assert!(j.get("remaining_tokens").is_none());
    assert_eq!(j["reset_requests"], "2s");
    assert!(j.get("reset_tokens").is_none());
}

#[test]
fn test_rate_limit_info_empty_headers() {
    let headers: Vec<(String, String)> = vec![];
    let info = RateLimitInfo::from_headers(&headers);

    assert_eq!(info.limit_requests, None);
    assert_eq!(info.limit_tokens, None);
    assert_eq!(info.remaining_requests, None);
    assert_eq!(info.remaining_tokens, None);
    assert_eq!(info.reset_requests, None);
    assert_eq!(info.reset_tokens, None);
}

#[test]
fn test_extra_headers_empty() {
    let adapter = GroqAdapter::new();
    assert!(adapter.extra_headers().is_empty());
}

#[test]
fn test_convert_response_with_tool_calls() {
    let adapter = GroqAdapter::new();
    let response = json!({
        "id": "chatcmpl-456",
        "object": "chat.completion",
        "model": "llama3-70b-8192",
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
    // Should pass through unchanged
    assert_eq!(result, response);
}
