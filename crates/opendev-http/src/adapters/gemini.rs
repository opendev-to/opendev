//! Google Gemini adapter.
//!
//! Converts the internal Chat Completions format to the Gemini
//! `generateContent` API and maps responses back.
//!
//! Key differences from OpenAI Chat Completions:
//! - Uses `contents` array with `parts` instead of `messages`
//! - System instruction is a separate top-level field
//! - Tool calls use `functionCall` / `functionResponse`
//! - Endpoint: `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent`

use serde_json::{Value, json};

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Adapter for the Google Gemini `generateContent` API.
#[derive(Debug, Clone)]
pub struct GeminiAdapter {
    base_url: String,
    model: String,
}

impl GeminiAdapter {
    /// Create a new Gemini adapter for the given model.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            model: model.into(),
        }
    }

    /// Create with a custom base URL (for proxies, etc.).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    // ── Request conversion: Chat Completions → Gemini ──────────────────

    /// Convert messages to Gemini `contents` array, extracting system instruction.
    fn convert_messages(messages: &[Value]) -> (Option<String>, Vec<Value>) {
        let mut system_text: Option<String> = None;
        let mut contents: Vec<Value> = Vec::new();

        for msg in messages {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            match role {
                "system" => {
                    let text = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                    system_text = Some(text.to_string());
                }
                "user" => {
                    let parts = Self::content_to_parts(msg.get("content"));
                    contents.push(json!({
                        "role": "user",
                        "parts": parts,
                    }));
                }
                "assistant" => {
                    let mut parts: Vec<Value> = Vec::new();

                    // Text content
                    if let Some(text) = msg.get("content").and_then(|c| c.as_str())
                        && !text.is_empty()
                    {
                        parts.push(json!({"text": text}));
                    }

                    // Tool calls → functionCall parts
                    if let Some(tool_calls) = msg.get("tool_calls").and_then(|tc| tc.as_array()) {
                        for tc in tool_calls {
                            let func = tc.get("function").cloned().unwrap_or(json!({}));
                            let args_str = func
                                .get("arguments")
                                .and_then(|a| a.as_str())
                                .unwrap_or("{}");
                            let args: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                            parts.push(json!({
                                "functionCall": {
                                    "name": func.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                                    "args": args,
                                }
                            }));
                        }
                    }

                    if !parts.is_empty() {
                        contents.push(json!({
                            "role": "model",
                            "parts": parts,
                        }));
                    }
                }
                "tool" => {
                    // Tool results → functionResponse in a user turn
                    let name = msg
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("function");
                    let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                    // Try to parse content as JSON for structured response
                    let response_val: Value = serde_json::from_str(content)
                        .unwrap_or_else(|_| json!({"result": content}));

                    contents.push(json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": name,
                                "response": response_val,
                            }
                        }]
                    }));
                }
                _ => {}
            }
        }

        (system_text, contents)
    }

    /// Convert a content value (string or array of blocks) to Gemini parts.
    fn content_to_parts(content: Option<&Value>) -> Vec<Value> {
        match content {
            Some(Value::String(s)) => vec![json!({"text": s})],
            Some(Value::Array(blocks)) => {
                blocks
                    .iter()
                    .filter_map(|block| {
                        let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match block_type {
                            "text" => {
                                let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
                                Some(json!({"text": text}))
                            }
                            "image_url" => {
                                // data:mime;base64,data → inlineData
                                if let Some(url) = block
                                    .get("image_url")
                                    .and_then(|iu| iu.get("url"))
                                    .and_then(|u| u.as_str())
                                    && let Some(rest) = url.strip_prefix("data:")
                                    && let Some((mime, data)) = rest.split_once(";base64,")
                                {
                                    return Some(json!({
                                        "inlineData": {
                                            "mimeType": mime,
                                            "data": data,
                                        }
                                    }));
                                }
                                None
                            }
                            _ => Some(json!({"text": block.to_string()})),
                        }
                    })
                    .collect()
            }
            _ => vec![json!({"text": ""})],
        }
    }

    /// Convert Chat Completions tool definitions to Gemini function declarations.
    fn convert_tools(tools: &[Value]) -> Vec<Value> {
        tools
            .iter()
            .filter_map(|tool| {
                let func = tool.get("function")?;
                Some(json!({
                    "name": func.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                    "description": func.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                    "parameters": func.get("parameters").cloned().unwrap_or(json!({"type": "object", "properties": {}})),
                }))
            })
            .collect()
    }

    // ── Response conversion: Gemini → Chat Completions ─────────────────

    /// Convert a Gemini generateContent response to Chat Completions format.
    fn response_to_chat_completions(&self, response: &Value) -> Value {
        let candidates = response
            .get("candidates")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        let candidate = candidates.first().cloned().unwrap_or(json!({}));
        let parts = candidate
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();

        // Extract text parts
        let text_parts: Vec<String> = parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()).map(String::from))
            .collect();

        // Extract function calls
        let tool_calls: Vec<Value> = parts
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let fc = p.get("functionCall")?;
                let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = fc.get("args").cloned().unwrap_or(json!({}));
                Some(json!({
                    "id": format!("call_{i}"),
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": serde_json::to_string(&args).unwrap_or_default(),
                    }
                }))
            })
            .collect();

        let content = if text_parts.is_empty() {
            Value::Null
        } else {
            Value::String(text_parts.join(""))
        };

        let finish_reason_raw = candidate
            .get("finishReason")
            .and_then(|r| r.as_str())
            .unwrap_or("STOP");

        let finish_reason = match finish_reason_raw {
            "STOP" => {
                if tool_calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                }
            }
            "MAX_TOKENS" => "length",
            "SAFETY" => "content_filter",
            _ => "stop",
        };

        let mut message = json!({
            "role": "assistant",
            "content": content,
        });

        if !tool_calls.is_empty() {
            message["tool_calls"] = Value::Array(tool_calls);
        }

        // Usage
        let usage_meta = response.get("usageMetadata").cloned().unwrap_or(json!({}));
        let prompt_tokens = usage_meta
            .get("promptTokenCount")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        let completion_tokens = usage_meta
            .get("candidatesTokenCount")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);

        json!({
            "id": format!("gemini-{}", uuid::Uuid::new_v4()),
            "object": "chat.completion",
            "model": &self.model,
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": finish_reason,
            }],
            "usage": {
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "total_tokens": prompt_tokens + completion_tokens,
            },
        })
    }
}

impl Default for GeminiAdapter {
    fn default() -> Self {
        Self::new("gemini-2.0-flash")
    }
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for GeminiAdapter {
    fn provider_name(&self) -> &str {
        "gemini"
    }

    fn convert_request(&self, payload: Value) -> Value {
        let messages = payload
            .get("messages")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();

        let (system_instruction, contents) = Self::convert_messages(&messages);

        let mut gemini_payload = json!({
            "contents": contents,
        });

        if let Some(system) = system_instruction {
            gemini_payload["systemInstruction"] = json!({
                "parts": [{"text": system}]
            });
        }

        // Generation config
        let mut gen_config = json!({});
        if let Some(temp) = payload.get("temperature") {
            gen_config["temperature"] = temp.clone();
        }
        if let Some(top_p) = payload.get("top_p") {
            gen_config["topP"] = top_p.clone();
        }
        let max_tok = payload
            .get("max_tokens")
            .or_else(|| payload.get("max_completion_tokens"));
        if let Some(tok) = max_tok {
            gen_config["maxOutputTokens"] = tok.clone();
        }
        if gen_config.as_object().is_some_and(|o| !o.is_empty()) {
            gemini_payload["generationConfig"] = gen_config;
        }

        // Tools
        if let Some(tools) = payload.get("tools").and_then(|t| t.as_array()) {
            let declarations = Self::convert_tools(tools);
            if !declarations.is_empty() {
                gemini_payload["tools"] = json!([{
                    "functionDeclarations": declarations,
                }]);
            }
        }

        gemini_payload
    }

    fn convert_response(&self, response: Value) -> Value {
        self.response_to_chat_completions(&response)
    }

    fn api_url(&self) -> &str {
        // The full URL includes the model name, built dynamically
        // Callers should use this base + model to construct the final URL
        // but for the adapter interface we return the full endpoint.
        // This is a computed value; we leak a string for the static lifetime workaround.
        // Instead, we store it. But the trait returns &str, so we need a stored value.
        // We'll return the base URL; the client builder should construct the full URL.
        &self.base_url
    }
}

/// Build the full Gemini API URL for a given model.
pub fn gemini_api_url(base_url: &str, model: &str) -> String {
    format!("{base_url}/models/{model}:generateContent")
}

#[cfg(test)]
mod tests {
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
}
