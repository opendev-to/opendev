//! Request transformation helpers for the OpenAI Responses API adapter.
//!
//! These functions convert an internal Chat Completions payload into the
//! format expected by the OpenAI Responses API.

use serde_json::{Value, json};

use super::{OpenAiAdapter, REASONING_PREFIXES};

impl OpenAiAdapter {
    /// Check if the model is a reasoning model (o1/o3).
    pub(super) fn is_reasoning_model(payload: &Value) -> bool {
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

    /// Convert messages array to Responses API `input` items and optional `instructions`.
    pub(super) fn convert_messages(messages: &[Value]) -> (Option<String>, Vec<Value>) {
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
    pub(super) fn convert_content_blocks(content: &Value) -> Value {
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
    pub(super) fn convert_tools(tools: &[Value]) -> Vec<Value> {
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
}
