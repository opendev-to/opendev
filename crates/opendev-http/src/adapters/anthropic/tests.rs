use super::*;
use crate::adapters::base::ProviderAdapter;

#[test]
fn test_provider_name() {
    let adapter = AnthropicAdapter::new();
    assert_eq!(adapter.provider_name(), "anthropic");
}

#[test]
fn test_api_url_default() {
    let adapter = AnthropicAdapter::new();
    assert_eq!(adapter.api_url(), DEFAULT_API_URL);
}

#[test]
fn test_api_url_custom() {
    let adapter = AnthropicAdapter::with_url("https://custom.api/v1/messages");
    assert_eq!(adapter.api_url(), "https://custom.api/v1/messages");
}

#[test]
fn test_extra_headers() {
    let adapter = AnthropicAdapter::new();
    let headers = adapter.extra_headers();
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "anthropic-version" && v == ANTHROPIC_VERSION)
    );
    assert!(headers.iter().any(|(k, v)| k == "anthropic-beta"
        && v.contains("prompt-caching-2024-07-31")
        && v.contains("interleaved-thinking-2025-05-14")));
}

#[test]
fn test_extra_headers_no_caching() {
    let adapter = AnthropicAdapter::new().with_caching(false);
    let headers = adapter.extra_headers();
    assert!(headers.iter().any(|(k, _)| k == "anthropic-version"));
    // Still has beta header for thinking
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "anthropic-beta" && v.contains("interleaved-thinking-2025-05-14"))
    );
}

#[test]
fn test_extract_system() {
    let mut payload = json!({
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"}
        ]
    });
    AnthropicAdapter::extract_system(&mut payload);
    assert_eq!(payload["system"], "You are helpful.");
    let messages = payload["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
}

#[test]
fn test_convert_image_blocks() {
    let mut payload = json!({
        "messages": [{
            "role": "user",
            "content": [{
                "type": "image_url",
                "image_url": {"url": "data:image/png;base64,abc123"}
            }]
        }]
    });
    AnthropicAdapter::convert_image_blocks(&mut payload);
    let block = &payload["messages"][0]["content"][0];
    assert_eq!(block["type"], "image");
    assert_eq!(block["source"]["type"], "base64");
    assert_eq!(block["source"]["media_type"], "image/png");
    assert_eq!(block["source"]["data"], "abc123");
}

#[test]
fn test_add_cache_control_string_content() {
    let mut payload = json!({
        "messages": [
            {"role": "user", "content": "Hello"}
        ]
    });
    AnthropicAdapter::add_cache_control(&mut payload);
    let content = &payload["messages"][0]["content"];
    assert!(content.is_array());
    assert_eq!(content[0]["cache_control"]["type"], "ephemeral");
}

#[test]
fn test_convert_request_removes_unsupported() {
    let adapter = AnthropicAdapter::new().with_caching(false);
    let payload = json!({
        "messages": [{"role": "user", "content": "Hi"}],
        "n": 1,
        "frequency_penalty": 0.5,
        "presence_penalty": 0.5,
        "logprobs": true
    });
    let result = adapter.convert_request(payload);
    assert!(result.get("n").is_none());
    assert!(result.get("frequency_penalty").is_none());
    assert!(result.get("presence_penalty").is_none());
    assert!(result.get("logprobs").is_none());
}

#[test]
fn test_response_to_chat_completions() {
    let response = json!({
        "id": "msg_123",
        "type": "message",
        "model": "claude-3-opus-20240229",
        "content": [{"type": "text", "text": "Hello!"}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 5}
    });
    let result = AnthropicAdapter::response_to_chat_completions(response);
    assert_eq!(result["object"], "chat.completion");
    assert_eq!(result["choices"][0]["message"]["content"], "Hello!");
    assert_eq!(result["choices"][0]["finish_reason"], "stop");
    assert_eq!(result["usage"]["prompt_tokens"], 10);
    assert_eq!(result["usage"]["completion_tokens"], 5);
    assert_eq!(result["usage"]["total_tokens"], 15);
}

#[test]
fn test_response_tool_use_finish_reason() {
    let response = json!({
        "id": "msg_456",
        "model": "claude-3",
        "content": [{"type": "text", "text": "Using tool"}],
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 0, "output_tokens": 0}
    });
    let result = AnthropicAdapter::response_to_chat_completions(response);
    assert_eq!(result["choices"][0]["finish_reason"], "tool_calls");
}

#[test]
fn test_response_extracts_thinking_blocks() {
    let response = json!({
        "id": "msg_789",
        "model": "claude-sonnet-4-20250514",
        "content": [
            {"type": "thinking", "thinking": "Let me think about this..."},
            {"type": "thinking", "thinking": "Step 2 of thinking"},
            {"type": "text", "text": "The answer is 42."}
        ],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 50}
    });
    let result = AnthropicAdapter::response_to_chat_completions(response);
    assert_eq!(
        result["choices"][0]["message"]["content"],
        "The answer is 42."
    );
    assert_eq!(
        result["choices"][0]["message"]["reasoning_content"],
        "Let me think about this...\n\nStep 2 of thinking"
    );
}

#[test]
fn test_response_no_thinking_blocks() {
    let response = json!({
        "id": "msg_100",
        "model": "claude-3-opus",
        "content": [{"type": "text", "text": "Hello!"}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 5, "output_tokens": 3}
    });
    let result = AnthropicAdapter::response_to_chat_completions(response);
    assert!(
        result["choices"][0]["message"]
            .get("reasoning_content")
            .is_none()
    );
}

#[test]
fn test_supports_thinking() {
    assert!(supports_thinking("claude-3-7-sonnet-20250219"));
    assert!(supports_thinking("claude-3.7-sonnet"));
    assert!(supports_thinking("claude-4-opus-20250514"));
    assert!(supports_thinking("claude-opus-4-20250514"));
    assert!(supports_thinking("claude-sonnet-4-20250514"));
    assert!(!supports_thinking("claude-3-opus-20240229"));
    assert!(!supports_thinking("claude-3-5-sonnet"));
    assert!(!supports_thinking("gpt-4o"));
}

#[test]
fn test_convert_request_with_thinking() {
    let adapter = AnthropicAdapter::new().with_caching(false);
    let payload = json!({
        "model": "claude-sonnet-4-20250514",
        "messages": [{"role": "user", "content": "Think about this"}],
        "_reasoning_effort": "medium"
    });
    let result = adapter.convert_request(payload);
    assert_eq!(result["thinking"]["type"], "enabled");
    assert_eq!(result["thinking"]["budget_tokens"], 16000);
    assert_eq!(result["temperature"], 1);
    // _reasoning_effort should be stripped
    assert!(result.get("_reasoning_effort").is_none());
}

#[test]
fn test_convert_request_thinking_unsupported_model() {
    let adapter = AnthropicAdapter::new().with_caching(false);
    let payload = json!({
        "model": "claude-3-opus-20240229",
        "messages": [{"role": "user", "content": "Hello"}],
        "_reasoning_effort": "high"
    });
    let result = adapter.convert_request(payload);
    assert!(result.get("thinking").is_none());
}

#[test]
fn test_convert_tool_messages_echoes_thinking() {
    let mut payload = json!({
        "messages": [
            {
                "role": "assistant",
                "content": "Let me read that file.",
                "reasoning_content": "I should read the file first.",
                "tool_calls": [{
                    "id": "tc-1",
                    "function": {"name": "read_file", "arguments": "{\"path\": \"test.rs\"}"}
                }]
            },
            {
                "role": "tool",
                "tool_call_id": "tc-1",
                "content": "file contents"
            }
        ]
    });
    AnthropicAdapter::convert_tool_messages(&mut payload);
    let messages = payload["messages"].as_array().unwrap();
    let assistant_content = messages[0]["content"].as_array().unwrap();
    // First block should be thinking
    assert_eq!(assistant_content[0]["type"], "thinking");
    assert_eq!(
        assistant_content[0]["thinking"],
        "I should read the file first."
    );
    // Then text, then tool_use
    assert_eq!(assistant_content[1]["type"], "text");
    assert_eq!(assistant_content[2]["type"], "tool_use");
}

#[test]
fn test_convert_request_thinking_ensures_min_max_tokens() {
    let adapter = AnthropicAdapter::new().with_caching(false);
    let payload = json!({
        "model": "claude-sonnet-4-20250514",
        "messages": [{"role": "user", "content": "Think"}],
        "_reasoning_effort": "high",
        "max_tokens": 1024
    });
    let result = adapter.convert_request(payload);
    // budget_tokens for "high" is 31999, so max_tokens should be at least 33023
    assert!(result["max_tokens"].as_u64().unwrap() >= 33023);
}

#[test]
fn test_supports_adaptive_thinking() {
    // 4.6 models support adaptive thinking
    assert!(supports_adaptive_thinking("claude-opus-4-6-20260301"));
    assert!(supports_adaptive_thinking("claude-opus-4.6-20260301"));
    assert!(supports_adaptive_thinking("claude-sonnet-4-6-20260301"));
    assert!(supports_adaptive_thinking("claude-sonnet-4.6-20260301"));
    // Non-4.6 models do not
    assert!(!supports_adaptive_thinking("claude-sonnet-4-20250514"));
    assert!(!supports_adaptive_thinking("claude-opus-4-20250514"));
    assert!(!supports_adaptive_thinking("claude-3-7-sonnet-20250219"));
    assert!(!supports_adaptive_thinking("gpt-4o"));
}

#[test]
fn test_convert_request_adaptive_thinking_high() {
    let adapter = AnthropicAdapter::new().with_caching(false);
    let payload = json!({
        "model": "claude-opus-4-6-20260301",
        "messages": [{"role": "user", "content": "Think deeply"}],
        "_reasoning_effort": "high"
    });
    let result = adapter.convert_request(payload);
    assert_eq!(result["thinking"]["type"], "adaptive");
    // "high" should be uncapped — no budget_tokens field
    assert!(result["thinking"].get("budget_tokens").is_none());
    assert_eq!(result["temperature"], 1);
}

#[test]
fn test_convert_request_adaptive_thinking_medium() {
    let adapter = AnthropicAdapter::new().with_caching(false);
    let payload = json!({
        "model": "claude-sonnet-4.6-20260301",
        "messages": [{"role": "user", "content": "Think"}],
        "_reasoning_effort": "medium"
    });
    let result = adapter.convert_request(payload);
    assert_eq!(result["thinking"]["type"], "adaptive");
    // Anthropic rejects `budget_tokens` under `thinking.adaptive`
    // ("Extra inputs are not permitted") — adaptive means the model chooses.
    assert!(
        result["thinking"].get("budget_tokens").is_none(),
        "adaptive thinking must not include budget_tokens; got {:?}",
        result["thinking"]
    );
}

#[test]
fn test_convert_request_adaptive_thinking_low() {
    let adapter = AnthropicAdapter::new().with_caching(false);
    let payload = json!({
        "model": "claude-opus-4.6-20260301",
        "messages": [{"role": "user", "content": "Quick"}],
        "_reasoning_effort": "low"
    });
    let result = adapter.convert_request(payload);
    assert_eq!(result["thinking"]["type"], "adaptive");
    assert!(
        result["thinking"].get("budget_tokens").is_none(),
        "adaptive thinking must not include budget_tokens; got {:?}",
        result["thinking"]
    );
}

#[test]
fn test_adaptive_thinking_never_sends_budget_tokens() {
    // Regression: Anthropic's Messages API rejects any payload where
    // `thinking.type == "adaptive"` contains `budget_tokens`, with
    // `thinking.adaptive.budget_tokens: Extra inputs are not permitted`.
    // Cover every effort level on every adaptive-capable model variant.
    let adapter = AnthropicAdapter::new().with_caching(false);
    let models = [
        "claude-opus-4-6-20260301",
        "claude-opus-4.6-20260301",
        "claude-sonnet-4-6-20260301",
        "claude-sonnet-4.6-20260301",
    ];
    let efforts = ["low", "medium", "high"];
    for model in models {
        for effort in efforts {
            let payload = json!({
                "model": model,
                "messages": [{"role": "user", "content": "Hi"}],
                "_reasoning_effort": effort,
            });
            let result = adapter.convert_request(payload);
            assert_eq!(
                result["thinking"]["type"], "adaptive",
                "{model} @ {effort}: expected adaptive"
            );
            assert!(
                result["thinking"].get("budget_tokens").is_none(),
                "{model} @ {effort}: adaptive payload must omit budget_tokens, got {:?}",
                result["thinking"]
            );
        }
    }
}

#[test]
fn test_thinking_blocks_signature_preserved_in_response() {
    let response = json!({
        "id": "msg_sig",
        "model": "claude-opus-4-6-20260301",
        "content": [
            {"type": "thinking", "thinking": "Deep thought", "signature": "sig_abc123"},
            {"type": "text", "text": "Answer."}
        ],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 20}
    });
    let result = AnthropicAdapter::response_to_chat_completions(response);
    let msg = &result["choices"][0]["message"];
    assert_eq!(msg["reasoning_content"], "Deep thought");
    // Raw _thinking_blocks should preserve the signature field
    let blocks = msg["_thinking_blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["signature"], "sig_abc123");
    assert_eq!(blocks[0]["thinking"], "Deep thought");
}

#[test]
fn test_thinking_blocks_signature_roundtrip() {
    // Simulate a multi-turn conversation: response -> echo-back
    // The _thinking_blocks with signatures should be used for echo-back
    let mut payload = json!({
        "messages": [
            {
                "role": "assistant",
                "content": "Using a tool.",
                "reasoning_content": "Let me think.",
                "_thinking_blocks": [
                    {"type": "thinking", "thinking": "Let me think.", "signature": "sig_xyz"}
                ],
                "tool_calls": [{
                    "id": "tc-1",
                    "function": {"name": "bash", "arguments": "{\"command\": \"ls\"}"}
                }]
            },
            {
                "role": "tool",
                "tool_call_id": "tc-1",
                "content": "file1.rs"
            }
        ]
    });
    AnthropicAdapter::convert_tool_messages(&mut payload);
    let messages = payload["messages"].as_array().unwrap();
    let assistant_content = messages[0]["content"].as_array().unwrap();
    // Should use raw block with signature, not reconstructed thinking
    assert_eq!(assistant_content[0]["type"], "thinking");
    assert_eq!(assistant_content[0]["signature"], "sig_xyz");
    assert_eq!(assistant_content[0]["thinking"], "Let me think.");
    assert_eq!(assistant_content[1]["type"], "text");
    assert_eq!(assistant_content[2]["type"], "tool_use");
}

#[test]
fn test_parse_stream_event_signature_delta() {
    let adapter = AnthropicAdapter::new();
    // Verify signature_delta content_block_delta emits ThinkingSignature
    let data = serde_json::json!({
        "type": "content_block_delta",
        "index": 0,
        "delta": {
            "type": "signature_delta",
            "signature": "EucBtest_signature_abc123"
        }
    });
    let event = adapter.parse_stream_event("content_block_delta", &data);
    match event {
        Some(crate::streaming::StreamEvent::ThinkingSignature { index, signature }) => {
            assert_eq!(index, 0);
            assert_eq!(signature, "EucBtest_signature_abc123");
        }
        other => panic!("Expected ThinkingSignature, got {:?}", other),
    }
}

#[test]
fn test_parse_stream_event_thinking_block_start() {
    let adapter = AnthropicAdapter::new();
    let data = serde_json::json!({
        "type": "content_block_start",
        "index": 0,
        "content_block": {
            "type": "thinking",
            "thinking": "",
            "signature": ""
        }
    });
    let event = adapter.parse_stream_event("content_block_start", &data);
    match event {
        Some(crate::streaming::StreamEvent::ThinkingBlockStart { index, signature }) => {
            assert_eq!(index, 0);
            // signature is Some("") from the empty placeholder in content_block_start
            assert_eq!(signature, Some("".to_string()));
        }
        other => panic!("Expected ThinkingBlockStart, got {:?}", other),
    }
}
