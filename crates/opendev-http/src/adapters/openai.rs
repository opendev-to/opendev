//! OpenAI Responses API adapter.
//!
//! The Responses API (`/v1/responses`) is OpenAI's recommended replacement for
//! Chat Completions.  This adapter transparently converts the internal
//! Chat Completions-shaped payload to the Responses API format and converts
//! responses back so the rest of the agent code is unaffected.
//!
//! See: https://platform.openai.com/docs/guides/migrate-to-responses

use serde_json::{Value, json};

const DEFAULT_API_URL: &str = "https://api.openai.com/v1/responses";

/// Set of model prefixes that are reasoning models (o1, o3).
const REASONING_PREFIXES: &[&str] = &["o1", "o3"];

/// Adapter for the OpenAI Responses API.
///
/// Converts internal Chat Completions payloads to the Responses API format
/// and converts responses back to Chat Completions format.
#[derive(Debug, Clone)]
pub struct OpenAiAdapter {
    api_url: String,
}

impl OpenAiAdapter {
    /// Create a new OpenAI adapter with the default Responses API URL.
    pub fn new() -> Self {
        Self {
            api_url: DEFAULT_API_URL.to_string(),
        }
    }

    /// Create with a custom API URL (for Azure, proxies, etc.).
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            api_url: url.into(),
        }
    }

    /// Check if the model is a reasoning model (o1/o3).
    fn is_reasoning_model(payload: &Value) -> bool {
        payload
            .get("model")
            .and_then(|m| m.as_str())
            .map(|model| {
                REASONING_PREFIXES
                    .iter()
                    .any(|prefix| model.starts_with(prefix))
            })
            .unwrap_or(false)
    }

    // ── Request conversion: Chat Completions → Responses API ────────────

    /// Convert messages array to Responses API `input` items and optional `instructions`.
    fn convert_messages(messages: &[Value]) -> (Option<String>, Vec<Value>) {
        let mut instructions: Option<String> = None;
        let mut input_items: Vec<Value> = Vec::new();

        for msg in messages {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            match role {
                "system" => {
                    instructions = msg
                        .get("content")
                        .and_then(|c| c.as_str())
                        .map(String::from);
                }
                "user" => {
                    let content = msg.get("content").cloned().unwrap_or(json!(""));
                    input_items.push(json!({
                        "type": "message",
                        "role": "user",
                        "content": Self::convert_content_blocks(&content),
                    }));
                }
                "assistant" => {
                    // Text content → message item
                    if let Some(content) = msg.get("content")
                        && content.is_string()
                        && !content.as_str().unwrap_or("").is_empty()
                    {
                        input_items.push(json!({
                            "type": "message",
                            "role": "assistant",
                            "content": content,
                        }));
                    }
                    // Tool calls → function_call items
                    if let Some(tool_calls) = msg.get("tool_calls").and_then(|tc| tc.as_array()) {
                        for tc in tool_calls {
                            let func = tc.get("function").cloned().unwrap_or(json!({}));
                            input_items.push(json!({
                                "type": "function_call",
                                "call_id": tc.get("id").and_then(|i| i.as_str()).unwrap_or(""),
                                "name": func.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                                "arguments": func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}"),
                            }));
                        }
                    }
                }
                "tool" => {
                    input_items.push(json!({
                        "type": "function_call_output",
                        "call_id": msg.get("tool_call_id").and_then(|i| i.as_str()).unwrap_or(""),
                        "output": msg.get("content").and_then(|c| c.as_str()).unwrap_or(""),
                    }));
                }
                _ => {}
            }
        }

        (instructions, input_items)
    }

    /// Convert content blocks from internal (Anthropic-like) format to Responses API format.
    ///
    /// - `{"type": "text", ...}` → `{"type": "input_text", ...}`
    /// - `{"type": "image", "source": {...}}` → `{"type": "input_image", "image_url": "data:...;base64,..."}`
    ///
    /// If content is a plain string, it is returned unchanged.
    fn convert_content_blocks(content: &Value) -> Value {
        match content {
            Value::String(_) => content.clone(),
            Value::Array(blocks) => {
                let converted: Vec<Value> = blocks
                    .iter()
                    .map(|block| {
                        let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match block_type {
                            "text" => {
                                json!({
                                    "type": "input_text",
                                    "text": block.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                                })
                            }
                            "image" => {
                                let source = block.get("source").cloned().unwrap_or(json!({}));
                                let media_type = source
                                    .get("media_type")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("image/png");
                                let data = source
                                    .get("data")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or("");
                                json!({
                                    "type": "input_image",
                                    "image_url": format!("data:{media_type};base64,{data}"),
                                })
                            }
                            _ => block.clone(),
                        }
                    })
                    .collect();
                Value::Array(converted)
            }
            _ => content.clone(),
        }
    }

    /// Flatten Chat Completions tool definitions to Responses API format.
    ///
    /// `{type: "function", function: {name, description, parameters}}`
    /// → `{type: "function", name, description, parameters}`
    fn convert_tools(tools: &[Value]) -> Vec<Value> {
        tools
            .iter()
            .filter_map(|tool| {
                if tool.get("type").and_then(|t| t.as_str()) == Some("function") {
                    let func = tool.get("function")?;
                    Some(json!({
                        "type": "function",
                        "name": func.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                        "description": func.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        "parameters": func.get("parameters").cloned().unwrap_or(json!({})),
                    }))
                } else {
                    None
                }
            })
            .collect()
    }

    // ── Response conversion: Responses API → Chat Completions ───────────

    /// Convert a Responses API output back to Chat Completions format.
    fn build_chat_completion(responses_data: &Value) -> Value {
        let output_items = responses_data
            .get("output")
            .and_then(|o| o.as_array())
            .cloned()
            .unwrap_or_default();

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<Value> = Vec::new();
        let mut reasoning_parts: Vec<String> = Vec::new();

        for item in &output_items {
            let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match item_type {
                "message" => {
                    // Content can be a string or list of content blocks
                    match item.get("content") {
                        Some(Value::Array(blocks)) => {
                            for block in blocks {
                                if block.get("type").and_then(|t| t.as_str()) == Some("output_text")
                                {
                                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                        text_parts.push(text.to_string());
                                    }
                                } else if let Some(s) = block.as_str() {
                                    text_parts.push(s.to_string());
                                }
                            }
                        }
                        Some(Value::String(s)) => {
                            text_parts.push(s.clone());
                        }
                        _ => {}
                    }
                }
                "function_call" => {
                    tool_calls.push(json!({
                        "id": item.get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(|i| i.as_str())
                            .unwrap_or(""),
                        "type": "function",
                        "function": {
                            "name": item.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                            "arguments": item.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}"),
                        },
                    }));
                }
                "reasoning" => {
                    if let Some(summary) = item.get("summary").and_then(|s| s.as_array()) {
                        for s in summary {
                            if let Some(text) = s.get("text").and_then(|t| t.as_str()) {
                                reasoning_parts.push(text.to_string());
                            } else if let Some(text) = s.as_str() {
                                reasoning_parts.push(text.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let content = if text_parts.is_empty() {
            Value::Null
        } else {
            Value::String(text_parts.join("\n"))
        };

        let mut message = json!({
            "role": "assistant",
            "content": content,
        });

        if !reasoning_parts.is_empty() {
            message["reasoning_content"] = Value::String(reasoning_parts.join("\n"));
        }

        if !tool_calls.is_empty() {
            message["tool_calls"] = Value::Array(tool_calls.clone());
        }

        // Determine finish_reason
        let finish_reason = if !tool_calls.is_empty() {
            "tool_calls"
        } else if responses_data.get("status").and_then(|s| s.as_str()) == Some("incomplete") {
            "length"
        } else {
            "stop"
        };

        // Usage conversion
        let usage_raw = responses_data.get("usage").cloned().unwrap_or(json!({}));
        let input_tokens = usage_raw
            .get("input_tokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        let output_tokens = usage_raw
            .get("output_tokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);

        json!({
            "id": responses_data.get("id").and_then(|i| i.as_str()).unwrap_or(""),
            "object": "chat.completion",
            "model": responses_data.get("model").and_then(|m| m.as_str()).unwrap_or(""),
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": finish_reason,
            }],
            "usage": {
                "prompt_tokens": input_tokens,
                "completion_tokens": output_tokens,
                "total_tokens": input_tokens + output_tokens,
            },
        })
    }
}

impl Default for OpenAiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for OpenAiAdapter {
    fn provider_name(&self) -> &str {
        "openai"
    }

    fn convert_request(&self, payload: Value) -> Value {
        let mut payload = payload;

        // Extract and remove internal reasoning effort field
        let reasoning_effort = payload
            .as_object_mut()
            .and_then(|obj| obj.remove("_reasoning_effort"))
            .and_then(|v| v.as_str().map(String::from));

        let messages = payload
            .get("messages")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();

        let (instructions, input_items) = Self::convert_messages(&messages);

        let mut responses_payload = json!({
            "model": payload.get("model").cloned().unwrap_or(json!("")),
            "input": input_items,
            "store": false,
        });

        if let Some(instr) = instructions {
            responses_payload["instructions"] = Value::String(instr);
        }

        // max_tokens / max_completion_tokens → max_output_tokens
        let max_tok = payload
            .get("max_completion_tokens")
            .or_else(|| payload.get("max_tokens"));
        if let Some(tok) = max_tok {
            responses_payload["max_output_tokens"] = tok.clone();
        }

        // Temperature (strip for reasoning models)
        if !Self::is_reasoning_model(&payload)
            && let Some(temp) = payload.get("temperature")
        {
            responses_payload["temperature"] = temp.clone();
        }

        // Reasoning config — always request when effort is configured.
        // Works for o-series, GPT-5+, and any future reasoning-capable models.
        // Non-reasoning models will simply not return reasoning output.
        if let Some(ref effort) = reasoning_effort {
            responses_payload["reasoning"] = json!({
                "effort": effort,
                "summary": "auto",
            });
        }

        // Tools
        if let Some(tools) = payload.get("tools").and_then(|t| t.as_array()) {
            responses_payload["tools"] = Value::Array(Self::convert_tools(tools));
        }

        responses_payload
    }

    fn convert_response(&self, response: Value) -> Value {
        Self::build_chat_completion(&response)
    }

    fn api_url(&self) -> &str {
        &self.api_url
    }
}

#[cfg(test)]
mod tests {
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
}
