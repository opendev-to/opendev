//! Request transformation for Amazon Bedrock.
//!
//! Converts OpenAI Chat Completions payloads to Bedrock's Anthropic-style format.

use serde_json::{Value, json};

/// Extract system message from messages array into a top-level field.
///
/// Bedrock's Anthropic format expects system as a separate top-level field.
pub(super) fn extract_system(payload: &mut Value) {
    if let Some(messages) = payload.get_mut("messages").and_then(|m| m.as_array_mut()) {
        let mut system_parts: Vec<String> = Vec::new();
        messages.retain(|msg| {
            if msg.get("role").and_then(|r| r.as_str()) == Some("system") {
                if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                    system_parts.push(content.to_string());
                }
                false
            } else {
                true
            }
        });

        if !system_parts.is_empty() {
            let combined = system_parts.join("\n\n");
            payload["system"] = json!(combined);
        }
    }
}

/// Convert Chat Completions tool schemas to Bedrock/Anthropic format.
///
/// OpenAI: `[{type: "function", function: {name, description, parameters}}]`
/// Bedrock: `[{name, description, input_schema}]`
pub(super) fn convert_tools(payload: &mut Value) {
    if let Some(tools) = payload.get_mut("tools").and_then(|t| t.as_array_mut()) {
        let converted: Vec<Value> = tools
            .iter()
            .filter_map(|tool| {
                let func = tool.get("function")?;
                Some(json!({
                    "name": func.get("name")?,
                    "description": func.get("description").cloned().unwrap_or(json!("")),
                    "input_schema": func.get("parameters").cloned()
                        .unwrap_or(json!({"type": "object", "properties": {}}))
                }))
            })
            .collect();
        if let Some(tools_slot) = payload.get_mut("tools") {
            *tools_slot = json!(converted);
        }
    }
}

/// Convert tool result messages from Chat Completions to Bedrock format.
///
/// Converts `role: "tool"` messages to `role: "user"` with `tool_result` blocks,
/// and assistant `tool_calls` to `tool_use` content blocks.
pub(super) fn convert_tool_messages(payload: &mut Value) {
    if let Some(messages) = payload.get_mut("messages").and_then(|m| m.as_array_mut()) {
        let mut converted: Vec<Value> = Vec::new();

        for msg in messages.iter() {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");

            match role {
                "assistant" => {
                    if let Some(tool_calls) = msg.get("tool_calls").and_then(|tc| tc.as_array()) {
                        let mut content_blocks: Vec<Value> = Vec::new();

                        if let Some(text) = msg.get("content").and_then(|c| c.as_str())
                            && !text.is_empty()
                        {
                            content_blocks.push(json!({
                                "type": "text",
                                "text": text
                            }));
                        }

                        for tc in tool_calls {
                            let func = tc.get("function").cloned().unwrap_or(json!({}));
                            let args_str = func
                                .get("arguments")
                                .and_then(|a| a.as_str())
                                .unwrap_or("{}");
                            let args: Value = serde_json::from_str(args_str).unwrap_or(json!({}));

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
                    let tool_call_id = msg.get("tool_call_id").cloned().unwrap_or(json!(""));
                    let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

                    let result_block = json!({
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": content
                    });

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

/// Ensure max_tokens is set (required by Bedrock Anthropic models).
pub(super) fn ensure_max_tokens(payload: &mut Value) {
    if payload.get("max_tokens").is_none() {
        if let Some(val) = payload.get("max_completion_tokens").cloned() {
            if let Some(obj) = payload.as_object_mut() {
                obj.remove("max_completion_tokens");
            }
            payload["max_tokens"] = val;
        } else {
            payload["max_tokens"] = json!(4096);
        }
    }
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
