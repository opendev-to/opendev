//! Request transformation helpers for the Anthropic adapter.
//!
//! These functions convert an OpenAI Chat Completions payload into the
//! format expected by the Anthropic Messages API.

use serde_json::{Value, json};

use super::AnthropicAdapter;

impl AnthropicAdapter {
    /// Extract system message from messages array and put it at the top level.
    pub(super) fn extract_system(payload: &mut Value) {
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
    pub(super) fn convert_image_blocks(payload: &mut Value) {
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
    pub(super) fn add_cache_control(payload: &mut Value) {
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
    pub(super) fn convert_tools(payload: &mut Value) {
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
    pub(super) fn convert_tool_messages(payload: &mut Value) {
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

                            // Echo thinking blocks back (required by Anthropic API).
                            // Prefer raw _thinking_blocks which preserve `signature` fields;
                            // fall back to reconstructing from reasoning_content.
                            if let Some(raw_blocks) =
                                msg.get("_thinking_blocks").and_then(|b| b.as_array())
                                && !raw_blocks.is_empty()
                            {
                                content_blocks.extend(raw_blocks.iter().cloned());
                            } else if let Some(reasoning) =
                                msg.get("reasoning_content").and_then(|r| r.as_str())
                                && !reasoning.is_empty()
                            {
                                content_blocks.push(super::response::build_thinking_block(
                                    reasoning, None,
                                ));
                            }

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
                            // Non-tool-call assistant messages may also have reasoning
                            let has_raw_blocks = msg
                                .get("_thinking_blocks")
                                .and_then(|b| b.as_array())
                                .is_some_and(|a| !a.is_empty());
                            let has_reasoning = msg
                                .get("reasoning_content")
                                .and_then(|r| r.as_str())
                                .is_some_and(|s| !s.is_empty());

                            if has_raw_blocks || has_reasoning {
                                let text =
                                    msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                                let mut content_blocks: Vec<Value> = Vec::new();

                                if let Some(raw_blocks) =
                                    msg.get("_thinking_blocks").and_then(|b| b.as_array())
                                    && !raw_blocks.is_empty()
                                {
                                    content_blocks.extend(raw_blocks.iter().cloned());
                                } else if let Some(reasoning) =
                                    msg.get("reasoning_content").and_then(|r| r.as_str())
                                    && !reasoning.is_empty()
                                {
                                    content_blocks.push(super::response::build_thinking_block(
                                        reasoning, None,
                                    ));
                                }

                                if !text.is_empty() {
                                    content_blocks.push(json!({
                                        "type": "text",
                                        "text": text
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
    pub(super) fn ensure_max_tokens(payload: &mut Value) {
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
}
