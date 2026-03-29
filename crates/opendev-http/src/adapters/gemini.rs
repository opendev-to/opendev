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

        // Extract text parts (skip parts that are thought-only)
        let text_parts: Vec<String> = parts
            .iter()
            .filter_map(|p| {
                // Skip parts that have thought=true (Gemini thinking)
                if p.get("thought").and_then(|t| t.as_bool()) == Some(true) {
                    return None;
                }
                p.get("text").and_then(|t| t.as_str()).map(String::from)
            })
            .collect();

        // Extract thinking/reasoning content from thought parts
        let thinking_parts: Vec<String> = parts
            .iter()
            .filter_map(|p| {
                if p.get("thought").and_then(|t| t.as_bool()) == Some(true) {
                    p.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect();
        let reasoning_content = if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.join("\n\n"))
        };

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
        if let Some(ref reasoning) = reasoning_content {
            message["reasoning_content"] = Value::String(reasoning.clone());
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

    /// Check if the model supports native thinking (Gemini 2.5+).
    fn supports_thinking(model: &str) -> bool {
        model.contains("2.5") || model.contains("2-5")
    }

    /// Map reasoning effort level to a thinking budget token count.
    fn thinking_budget(effort: &str) -> u64 {
        match effort {
            "low" => 4000,
            "high" => 24576,
            _ => 16000, // medium
        }
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

        // Thinking config for Gemini 2.5+ models
        if Self::supports_thinking(&self.model)
            && let Some(ref effort) = reasoning_effort
        {
            gen_config["thinkingConfig"] = json!({
                "includeThoughts": true,
                "thinkingBudget": Self::thinking_budget(effort),
            });
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
        &self.base_url
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn enable_streaming(&self, _payload: &mut Value) {
        // Gemini doesn't use a payload flag — streaming is via URL endpoint
    }

    fn streaming_url(&self, base_url: &str) -> Option<String> {
        // Transform generateContent → streamGenerateContent?alt=sse
        Some(base_url.replace(":generateContent", ":streamGenerateContent?alt=sse"))
    }

    fn parse_stream_event(
        &self,
        _event_type: &str,
        data: &Value,
    ) -> Option<crate::streaming::StreamEvent> {
        use crate::streaming::StreamEvent;

        // Gemini streams partial generateContent responses as data: lines.
        // Each chunk has candidates[0].content.parts with text or thought parts.
        let candidates = data.get("candidates")?.as_array()?;
        let candidate = candidates.first()?;
        let parts = candidate.get("content")?.get("parts")?.as_array()?;

        for part in parts {
            let is_thought = part.get("thought").and_then(|t| t.as_bool()) == Some(true);
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                if is_thought {
                    return Some(StreamEvent::ReasoningDelta(text.to_string()));
                } else {
                    return Some(StreamEvent::TextDelta(text.to_string()));
                }
            }
        }

        // Check for error
        if let Some(error) = data.get("error") {
            let msg = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown Gemini error");
            return Some(StreamEvent::Error(msg.to_string()));
        }

        None
    }
}

/// Build the full Gemini API URL for a given model.
pub fn gemini_api_url(base_url: &str, model: &str) -> String {
    format!("{base_url}/models/{model}:generateContent")
}

#[cfg(test)]
#[path = "gemini_tests.rs"]
mod tests;
