//! Anthropic-specific adapter.
//!
//! Handles Anthropic API differences:
//! - Messages API format (system as top-level field, not in messages)
//! - `anthropic-version` header
//! - Prompt caching via `cache_control` blocks
//! - Image blocks using Anthropic's native `source` format

use serde_json::{Value, json};

const DEFAULT_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Adapter for the Anthropic Messages API.
#[derive(Debug, Clone)]
pub struct AnthropicAdapter {
    api_url: String,
    enable_caching: bool,
}

impl AnthropicAdapter {
    /// Create a new Anthropic adapter.
    pub fn new() -> Self {
        Self {
            api_url: DEFAULT_API_URL.to_string(),
            enable_caching: true,
        }
    }

    /// Create with a custom API URL.
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            api_url: url.into(),
            enable_caching: true,
        }
    }

    /// Enable or disable prompt caching.
    pub fn with_caching(mut self, enable: bool) -> Self {
        self.enable_caching = enable;
        self
    }

    /// Extract system message from messages array and put it at the top level.
    fn extract_system(payload: &mut Value) {
        if let Some(messages) = payload.get_mut("messages").and_then(|m| m.as_array_mut()) {
            let mut system_parts: Vec<Value> = Vec::new();
            messages.retain(|msg| {
                if msg.get("role").and_then(|r| r.as_str()) == Some("system") {
                    if let Some(content) = msg.get("content") {
                        system_parts.push(content.clone());
                    }
                    false
                } else {
                    true
                }
            });

            if !system_parts.is_empty() {
                // Combine into a single system string
                let combined: String = system_parts
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if !combined.is_empty() {
                    payload["system"] = json!(combined);
                }
            }
        }
    }

    /// Convert OpenAI-format image_url blocks to Anthropic source format.
    fn convert_image_blocks(payload: &mut Value) {
        if let Some(messages) = payload.get_mut("messages").and_then(|m| m.as_array_mut()) {
            for msg in messages.iter_mut() {
                if let Some(content) = msg.get_mut("content").and_then(|c| c.as_array_mut()) {
                    for block in content.iter_mut() {
                        if block.get("type").and_then(|t| t.as_str()) == Some("image_url")
                            && let Some(url) = block
                                .get("image_url")
                                .and_then(|iu| iu.get("url"))
                                .and_then(|u| u.as_str())
                        {
                            // Parse data:media_type;base64,data
                            if let Some(rest) = url.strip_prefix("data:")
                                && let Some((media_type, data)) = rest.split_once(";base64,")
                            {
                                *block = json!({
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": media_type,
                                        "data": data
                                    }
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Add cache_control to the last user message if caching is enabled.
    fn add_cache_control(payload: &mut Value) {
        if let Some(messages) = payload.get_mut("messages").and_then(|m| m.as_array_mut()) {
            // Find the last user message with content
            if let Some(last_user) = messages
                .iter_mut()
                .rev()
                .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
                && let Some(content) = last_user.get_mut("content")
            {
                if content.is_string() {
                    // Convert string content to block format with cache_control
                    let text = content.as_str().unwrap_or_default().to_string();
                    *content = json!([{
                        "type": "text",
                        "text": text,
                        "cache_control": {"type": "ephemeral"}
                    }]);
                } else if let Some(blocks) = content.as_array_mut() {
                    // Add cache_control to the last block
                    if let Some(last_block) = blocks.last_mut()
                        && let Some(obj) = last_block.as_object_mut()
                    {
                        obj.insert("cache_control".into(), json!({"type": "ephemeral"}));
                    }
                }
            }
        }
    }

    /// Convert Chat Completions tool schemas to Anthropic format.
    ///
    /// OpenAI: `[{type: "function", function: {name, description, parameters}}]`
    /// Anthropic: `[{name, description, input_schema}]`
    fn convert_tools(payload: &mut Value) {
        if let Some(tools) = payload.get_mut("tools").and_then(|t| t.as_array_mut()) {
            let converted: Vec<Value> = tools
                .iter()
                .filter_map(|tool| {
                    let func = tool.get("function")?;
                    Some(json!({
                        "name": func.get("name")?,
                        "description": func.get("description").cloned().unwrap_or(json!("")),
                        "input_schema": func.get("parameters").cloned().unwrap_or(json!({"type": "object", "properties": {}}))
                    }))
                })
                .collect();
            if let Some(tools_slot) = payload.get_mut("tools") {
                *tools_slot = json!(converted);
            }
        }

        // Convert tool_choice from Chat Completions to Anthropic format
        if let Some(tc) = payload.get("tool_choice").cloned()
            && let Some(tc_str) = tc.as_str()
        {
            match tc_str {
                "auto" => {
                    payload["tool_choice"] = json!({"type": "auto"});
                }
                "none" => {
                    // Anthropic doesn't have tool_choice "none" — just remove tools
                    if let Some(obj) = payload.as_object_mut() {
                        obj.remove("tools");
                        obj.remove("tool_choice");
                    }
                }
                "required" => {
                    payload["tool_choice"] = json!({"type": "any"});
                }
                _ => {}
            }
        }
    }

    /// Convert tool results in messages from Chat Completions to Anthropic format.
    ///
    /// Chat Completions uses `role: "tool"` messages. Anthropic expects
    /// `role: "user"` messages with `tool_result` content blocks.
    /// Also converts assistant `tool_calls` to Anthropic `tool_use` content blocks.
    fn convert_tool_messages(payload: &mut Value) {
        if let Some(messages) = payload.get_mut("messages").and_then(|m| m.as_array_mut()) {
            let mut converted: Vec<Value> = Vec::new();

            for msg in messages.iter() {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");

                match role {
                    "assistant" => {
                        // Convert tool_calls to Anthropic tool_use content blocks
                        if let Some(tool_calls) = msg.get("tool_calls").and_then(|tc| tc.as_array())
                        {
                            let mut content_blocks: Vec<Value> = Vec::new();

                            // Add text content if present
                            if let Some(text) = msg.get("content").and_then(|c| c.as_str())
                                && !text.is_empty()
                            {
                                content_blocks.push(json!({
                                    "type": "text",
                                    "text": text
                                }));
                            }

                            // Convert each tool_call to a tool_use block
                            for tc in tool_calls {
                                let func = tc.get("function").cloned().unwrap_or(json!({}));
                                let args_str = func
                                    .get("arguments")
                                    .and_then(|a| a.as_str())
                                    .unwrap_or("{}");
                                let args: Value =
                                    serde_json::from_str(args_str).unwrap_or(json!({}));

                                content_blocks.push(json!({
                                    "type": "tool_use",
                                    "id": tc.get("id").cloned().unwrap_or(json!("")),
                                    "name": func.get("name").cloned().unwrap_or(json!("")),
                                    "input": args
                                }));
                            }

                            converted.push(json!({
                                "role": "assistant",
                                "content": content_blocks
                            }));
                        } else {
                            converted.push(msg.clone());
                        }
                    }
                    "tool" => {
                        // Convert tool result to Anthropic user message with tool_result block
                        let tool_call_id = msg.get("tool_call_id").cloned().unwrap_or(json!(""));
                        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

                        // Merge consecutive tool results into one user message
                        let result_block = json!({
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": content
                        });

                        // Check if the last converted message is already a user tool_result
                        let should_merge = converted.last().is_some_and(|last| {
                            last.get("role").and_then(|r| r.as_str()) == Some("user")
                                && last.get("content").and_then(|c| c.as_array()).is_some_and(
                                    |blocks| {
                                        blocks.iter().all(|b| {
                                            b.get("type").and_then(|t| t.as_str())
                                                == Some("tool_result")
                                        })
                                    },
                                )
                        });

                        if should_merge {
                            if let Some(last) = converted.last_mut()
                                && let Some(blocks) =
                                    last.get_mut("content").and_then(|c| c.as_array_mut())
                            {
                                blocks.push(result_block);
                            }
                        } else {
                            converted.push(json!({
                                "role": "user",
                                "content": [result_block]
                            }));
                        }
                    }
                    _ => {
                        converted.push(msg.clone());
                    }
                }
            }

            if let Some(messages_slot) = payload.get_mut("messages") {
                *messages_slot = json!(converted);
            }
        }
    }

    /// Ensure max_tokens is set (required by Anthropic API).
    fn ensure_max_tokens(payload: &mut Value) {
        if payload.get("max_tokens").is_none() {
            // Check for max_completion_tokens (OpenAI o-series param) and convert
            if let Some(val) = payload.get("max_completion_tokens").cloned() {
                if let Some(obj) = payload.as_object_mut() {
                    obj.remove("max_completion_tokens");
                }
                payload["max_tokens"] = val;
            } else {
                payload["max_tokens"] = json!(16384);
            }
        }
    }

    /// Convert Anthropic response to Chat Completions format.
    fn response_to_chat_completions(response: Value) -> Value {
        let blocks = response
            .get("content")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        // Extract text content
        let content: String = blocks
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                    b.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        // Extract tool_use blocks → Chat Completions tool_calls
        let tool_calls: Vec<Value> = blocks
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    let id = b.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let input = b.get("input").cloned().unwrap_or(json!({}));
                    Some(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(&input).unwrap_or_default()
                        }
                    }))
                } else {
                    None
                }
            })
            .collect();

        let model = response
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");

        let usage = response.get("usage").cloned().unwrap_or(json!({}));
        let stop_reason = response
            .get("stop_reason")
            .and_then(|r| r.as_str())
            .unwrap_or("stop");

        let finish_reason = match stop_reason {
            "end_turn" => "stop",
            "max_tokens" => "length",
            "tool_use" => "tool_calls",
            other => other,
        };

        let mut message = json!({
            "role": "assistant",
            "content": content
        });

        if !tool_calls.is_empty() {
            message["tool_calls"] = json!(tool_calls);
        }

        json!({
            "id": response.get("id").cloned().unwrap_or(json!("")),
            "object": "chat.completion",
            "model": model,
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": finish_reason
            }],
            "usage": {
                "prompt_tokens": usage.get("input_tokens").cloned().unwrap_or(json!(0)),
                "completion_tokens": usage.get("output_tokens").cloned().unwrap_or(json!(0)),
                "total_tokens": usage.get("input_tokens").and_then(|i| i.as_u64())
                    .unwrap_or(0)
                    + usage.get("output_tokens").and_then(|o| o.as_u64())
                    .unwrap_or(0)
            }
        })
    }
}

impl Default for AnthropicAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for AnthropicAdapter {
    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn convert_request(&self, mut payload: Value) -> Value {
        Self::extract_system(&mut payload);
        Self::convert_image_blocks(&mut payload);
        Self::convert_tools(&mut payload);
        Self::convert_tool_messages(&mut payload);
        Self::ensure_max_tokens(&mut payload);

        if self.enable_caching {
            Self::add_cache_control(&mut payload);
        }

        // Remove unsupported fields
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("n");
            obj.remove("frequency_penalty");
            obj.remove("presence_penalty");
            obj.remove("logprobs");
        }

        payload
    }

    fn convert_response(&self, response: Value) -> Value {
        Self::response_to_chat_completions(response)
    }

    fn api_url(&self) -> &str {
        &self.api_url
    }

    fn extra_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![("anthropic-version".into(), ANTHROPIC_VERSION.into())];
        if self.enable_caching {
            headers.push(("anthropic-beta".into(), "prompt-caching-2024-07-31".into()));
        }
        headers
    }
}

#[cfg(test)]
mod tests {
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
        assert!(headers.iter().any(|(k, _)| k == "anthropic-beta"));
    }

    #[test]
    fn test_extra_headers_no_caching() {
        let adapter = AnthropicAdapter::new().with_caching(false);
        let headers = adapter.extra_headers();
        assert!(headers.iter().any(|(k, _)| k == "anthropic-version"));
        assert!(!headers.iter().any(|(k, _)| k == "anthropic-beta"));
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
}
