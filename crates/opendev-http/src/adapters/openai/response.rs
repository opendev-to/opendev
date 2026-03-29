//! Response transformation helpers for the OpenAI Responses API adapter.
//!
//! These functions convert OpenAI Responses API output back into the
//! internal Chat Completions format used throughout the codebase.

use serde_json::{Value, json};

use super::OpenAiAdapter;

impl OpenAiAdapter {
    /// Convert a Responses API output back to Chat Completions format.
    pub(super) fn build_chat_completion(responses_data: &Value) -> Value {
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
