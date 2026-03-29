use super::*;
use crate::adapters::base::ProviderAdapter;

#[test]
fn test_provider_name() {
    let adapter = BedrockAdapter::new("anthropic.claude-3-sonnet-20240229-v1:0");
    assert_eq!(adapter.provider_name(), "bedrock");
}

#[test]
fn test_api_url_format() {
    let adapter =
        BedrockAdapter::with_region("anthropic.claude-3-sonnet-20240229-v1:0", "us-west-2");
    assert_eq!(
        adapter.api_url(),
        "https://bedrock-runtime.us-west-2.amazonaws.com/model/anthropic.claude-3-sonnet-20240229-v1:0/invoke"
    );
}

#[test]
fn test_api_url_default_region() {
    let adapter =
        BedrockAdapter::with_region("anthropic.claude-3-haiku-20240307-v1:0", "us-east-1");
    assert!(adapter.api_url().contains("us-east-1"));
}

#[test]
fn test_model_id() {
    let adapter = BedrockAdapter::new("anthropic.claude-3-sonnet-20240229-v1:0");
    assert_eq!(
        adapter.model_id(),
        "anthropic.claude-3-sonnet-20240229-v1:0"
    );
}

#[test]
fn test_region() {
    let adapter = BedrockAdapter::with_region("model", "eu-west-1");
    assert_eq!(adapter.region(), "eu-west-1");
}

#[test]
fn test_convert_request_removes_unsupported_fields() {
    let adapter = BedrockAdapter::with_region("model-id", "us-east-1");
    let payload = json!({
        "model": "model-id",
        "messages": [{"role": "user", "content": "Hi"}],
        "n": 1,
        "frequency_penalty": 0.5,
        "presence_penalty": 0.5,
        "logprobs": true,
        "stream": true
    });
    let result = adapter.convert_request(payload);
    assert!(result.get("model").is_none());
    assert!(result.get("n").is_none());
    assert!(result.get("frequency_penalty").is_none());
    assert!(result.get("presence_penalty").is_none());
    assert!(result.get("logprobs").is_none());
    assert!(result.get("stream").is_none());
    assert_eq!(result["anthropic_version"], "bedrock-2023-05-31");
}

#[test]
fn test_convert_request_sets_max_tokens() {
    let adapter = BedrockAdapter::with_region("model-id", "us-east-1");
    let payload = json!({
        "messages": [{"role": "user", "content": "Hi"}]
    });
    let result = adapter.convert_request(payload);
    assert_eq!(result["max_tokens"], 4096);
}

#[test]
fn test_convert_request_preserves_custom_max_tokens() {
    let adapter = BedrockAdapter::with_region("model-id", "us-east-1");
    let payload = json!({
        "messages": [{"role": "user", "content": "Hi"}],
        "max_tokens": 8192
    });
    let result = adapter.convert_request(payload);
    assert_eq!(result["max_tokens"], 8192);
}

#[test]
fn test_convert_request_converts_max_completion_tokens() {
    let adapter = BedrockAdapter::with_region("model-id", "us-east-1");
    let payload = json!({
        "messages": [{"role": "user", "content": "Hi"}],
        "max_completion_tokens": 2048
    });
    let result = adapter.convert_request(payload);
    assert_eq!(result["max_tokens"], 2048);
    assert!(result.get("max_completion_tokens").is_none());
}

#[test]
fn test_extra_headers() {
    let adapter = BedrockAdapter::with_region("model-id", "us-east-1");
    let headers = adapter.extra_headers();
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "Content-Type" && v == "application/json")
    );
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "Accept" && v == "application/json")
    );
}

#[test]
fn test_build_url() {
    let url = BedrockAdapter::build_url("ap-southeast-1", "anthropic.claude-v2");
    assert_eq!(
        url,
        "https://bedrock-runtime.ap-southeast-1.amazonaws.com/model/anthropic.claude-v2/invoke"
    );
}
