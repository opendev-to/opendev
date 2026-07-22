use super::*;

#[test]
fn test_chatgpt_adapter_uses_only_trusted_stateless_responses_endpoint() {
    let adapter = OpenAiChatGptAdapter::new();
    assert_eq!(adapter.provider_name(), "openai-chatgpt");
    assert_eq!(adapter.api_url(), CHATGPT_CODEX_RESPONSES_URL);

    let converted = adapter.convert_request(serde_json::json!({
        "model": "codex-model",
        "messages": [{"role": "user", "content": "hello"}]
    }));
    assert_eq!(converted["store"], false);
    assert!(converted.get("previous_response_id").is_none());
}

#[test]
fn test_chatgpt_adapter_removes_platform_token_limit_and_uses_codex_reasoning_shape() {
    let adapter = OpenAiChatGptAdapter::new();
    let converted = adapter.convert_request(serde_json::json!({
        "model": "gpt-5.4",
        "max_completion_tokens": 128000,
        "_reasoning_effort": "medium",
        "messages": [{"role": "user", "content": "hello"}]
    }));

    assert!(converted.get("max_output_tokens").is_none());
    assert_eq!(converted["reasoning"]["effort"], "medium");
    assert_eq!(converted["reasoning"]["summary"], "auto");
    assert!(converted["include"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item == "reasoning.encrypted_content")
    }));
}
