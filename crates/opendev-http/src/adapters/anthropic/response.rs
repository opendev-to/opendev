//! Response transformation helpers for the Anthropic adapter.
//!
//! These functions convert Anthropic Messages API responses (both
//! non-streaming and SSE streaming) back into the OpenAI Chat Completions
//! format used internally.

use serde_json::{Value, json};

use super::AnthropicAdapter;

/// Build a single Anthropic `thinking` content block.
///
/// Used by both the streaming synthesizer (which has signatures from `signature_delta`)
/// and the request path's fallback (which only has plain reasoning text).
pub(crate) fn build_thinking_block(text: &str, signature: Option<&str>) -> Value {
    let mut b = json!({"type": "thinking", "thinking": text});
    if let Some(s) = signature {
        b["signature"] = Value::String(s.to_string());
    }
    b
}

impl AnthropicAdapter {
    /// Convert Anthropic response to Chat Completions format.
    pub(super) fn response_to_chat_completions(response: Value) -> Value {
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

        // Extract thinking blocks -> reasoning_content + raw _thinking_blocks for echo-back
        let thinking_blocks: Vec<Value> = blocks
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("thinking"))
            .cloned()
            .collect();
        let thinking_parts: Vec<String> = thinking_blocks
            .iter()
            .filter_map(|b| b.get("thinking").and_then(|t| t.as_str()).map(String::from))
            .collect();
        let reasoning_content = if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.join("\n\n"))
        };

        // Extract tool_use blocks -> Chat Completions tool_calls
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
        if let Some(ref reasoning) = reasoning_content {
            message["reasoning_content"] = json!(reasoning);
        }
        // Store raw thinking blocks (with signature fields) for multi-turn echo-back
        if !thinking_blocks.is_empty() {
            message["_thinking_blocks"] = json!(thinking_blocks);
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

    /// Parse a single SSE event from the Anthropic streaming API.
    pub(super) fn parse_stream_event_impl(
        &self,
        event_type: &str,
        data: &Value,
    ) -> Option<crate::streaming::StreamEvent> {
        use crate::streaming::StreamEvent;
        match event_type {
            "content_block_delta" => {
                let delta = data.get("delta")?;
                let delta_type = delta.get("type")?.as_str()?;
                match delta_type {
                    "text_delta" => {
                        let text = delta.get("text")?.as_str()?;
                        Some(StreamEvent::TextDelta(text.to_string()))
                    }
                    "thinking_delta" => {
                        let text = delta.get("thinking")?.as_str()?;
                        Some(StreamEvent::ReasoningDelta(text.to_string()))
                    }
                    "signature_delta" => {
                        let index =
                            data.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                        let signature = delta.get("signature")?.as_str()?.to_string();
                        Some(StreamEvent::ThinkingSignature { index, signature })
                    }
                    "input_json_delta" => {
                        let index =
                            data.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                        let partial = delta
                            .get("partial_json")
                            .and_then(|p| p.as_str())
                            .unwrap_or("")
                            .to_string();
                        Some(StreamEvent::FunctionCallDelta {
                            index,
                            delta: partial,
                        })
                    }
                    _ => None,
                }
            }
            "message_stop" => None,
            "message_start" => None,
            "message_delta" => {
                // Usage and stop_reason updates
                let usage = data.get("usage").cloned();
                let stop_reason = data
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(|s| s.as_str())
                    .map(String::from);
                if usage.is_some() || stop_reason.is_some() {
                    Some(StreamEvent::UsageUpdate { usage, stop_reason })
                } else {
                    None
                }
            }
            "content_block_start" => {
                let cb = data.get("content_block")?;
                let block_type = cb.get("type").and_then(|t| t.as_str())?;
                match block_type {
                    "thinking" => {
                        let index =
                            data.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                        let signature =
                            cb.get("signature").and_then(|s| s.as_str()).map(String::from);
                        Some(StreamEvent::ThinkingBlockStart { index, signature })
                    }
                    "tool_use" => {
                        let index =
                            data.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                        let call_id = cb
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = cb
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        Some(StreamEvent::FunctionCallStart {
                            index,
                            call_id,
                            name,
                            initial_args: None,
                        })
                    }
                    _ => None,
                }
            }
            "content_block_stop" => None,
            "ping" => None,
            "error" => {
                let error = data.get("error")?;
                let msg = error.get("message")?.as_str()?;
                Some(StreamEvent::Error(msg.to_string()))
            }
            _ => None,
        }
    }
}
