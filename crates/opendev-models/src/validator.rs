//! Message schema validation for session history.
//!
//! Validates messages before saving and repairs/filters on load to prevent
//! malformed messages from corrupting session history.

use crate::message::{ChatMessage, Role, ToolCall};
use tracing::warn;

/// Result of message validation.
#[derive(Debug, Clone)]
pub struct ValidationVerdict {
    pub is_valid: bool,
    pub reason: String,
}

impl ValidationVerdict {
    fn valid() -> Self {
        Self {
            is_valid: true,
            reason: String::new(),
        }
    }

    fn invalid(reason: impl Into<String>) -> Self {
        Self {
            is_valid: false,
            reason: reason.into(),
        }
    }
}

/// Check if a value is natively JSON-serializable.
fn is_json_serializable(value: &serde_json::Value) -> bool {
    // serde_json::Value is always serializable by definition
    serde_json::to_string(value).is_ok()
}

/// Validate a single tool call. Returns error reason or None if valid.
fn validate_tool_call(tc: &ToolCall, path: &str) -> Option<String> {
    let prefix = if path.is_empty() {
        "tool_call".to_string()
    } else {
        format!("{path}tool_call")
    };

    if tc.id.trim().is_empty() {
        return Some(format!("{prefix} has empty id"));
    }

    if tc.name.trim().is_empty() {
        return Some(format!("{prefix} [{}] has empty name", tc.id));
    }

    // A tool call must have result or error (except task_complete)
    if tc.result.is_none() && tc.error.is_none() && tc.name != "task_complete" {
        return Some(format!(
            "{prefix} [{}] ({}) has no result and no error",
            tc.id, tc.name
        ));
    }

    // Check result serializability
    if let Some(ref result) = tc.result
        && !is_json_serializable(result)
    {
        return Some(format!("{prefix} [{}] has non-serializable result", tc.id));
    }

    // Validate nested tool calls recursively
    for (i, nested) in tc.nested_tool_calls.iter().enumerate() {
        let nested_path = format!("{prefix}[{i}].");
        if let Some(reason) = validate_tool_call(nested, &nested_path) {
            return Some(reason);
        }
    }

    None
}

/// Strict pre-save validation of a message.
pub fn validate_message(msg: &ChatMessage) -> ValidationVerdict {
    match msg.role {
        Role::User => {
            if msg.content.trim().is_empty() {
                return ValidationVerdict::invalid("user message has empty content");
            }
            if !msg.tool_calls.is_empty() {
                return ValidationVerdict::invalid("user message has tool_calls");
            }
        }
        Role::Assistant => {
            let has_content = !msg.content.trim().is_empty();
            let has_tools = !msg.tool_calls.is_empty();
            if !has_content && !has_tools {
                return ValidationVerdict::invalid(
                    "assistant message has no content and no tool_calls",
                );
            }

            for tc in &msg.tool_calls {
                if let Some(reason) = validate_tool_call(tc, "") {
                    return ValidationVerdict::invalid(reason);
                }
            }

            if let Some(ref trace) = msg.thinking_trace
                && trace.trim().is_empty()
            {
                return ValidationVerdict::invalid("assistant message has empty thinking_trace");
            }
            if let Some(ref reasoning) = msg.reasoning_content
                && reasoning.trim().is_empty()
            {
                return ValidationVerdict::invalid("assistant message has empty reasoning_content");
            }
        }
        Role::System => {
            if msg.content.trim().is_empty() {
                return ValidationVerdict::invalid("system message has empty content");
            }
        }
    }

    // Token usage validation
    if let Some(ref usage) = msg.token_usage {
        let value = serde_json::to_value(usage).unwrap_or(serde_json::Value::Null);
        if !value.is_object() {
            return ValidationVerdict::invalid("token_usage is not a dict");
        }
    }

    ValidationVerdict::valid()
}

/// Repair a single tool call and return it.
fn repair_tool_call(tc: &mut ToolCall) {
    // Fix incomplete tool calls (no result and no error)
    if tc.result.is_none() && tc.error.is_none() && tc.name != "task_complete" {
        tc.error = Some("Tool execution was interrupted or never completed.".to_string());
    }

    // Repair nested tool calls recursively
    for nested in &mut tc.nested_tool_calls {
        repair_tool_call(nested);
    }
}

/// Attempt to repair a malformed message. Returns None if unrecoverable.
pub fn repair_message(msg: &mut ChatMessage) -> bool {
    let has_content = !msg.content.trim().is_empty();
    let has_tools = !msg.tool_calls.is_empty();

    // Drop completely empty messages
    if !has_content && !has_tools {
        return false;
    }

    // Repair tool calls
    for tc in &mut msg.tool_calls {
        repair_tool_call(tc);
    }

    // Normalize empty thinking_trace / reasoning_content to None
    if let Some(ref trace) = msg.thinking_trace
        && trace.trim().is_empty()
    {
        msg.thinking_trace = None;
    }
    if let Some(ref reasoning) = msg.reasoning_content
        && reasoning.trim().is_empty()
    {
        msg.reasoning_content = None;
    }

    // Fix non-serializable token_usage
    if let Some(ref usage) = msg.token_usage
        && serde_json::to_value(usage).is_err()
    {
        msg.token_usage = None;
    }

    true
}

/// Bulk load-time cleanup: repair what we can, drop what we can't.
pub fn filter_and_repair_messages(messages: &mut Vec<ChatMessage>) -> (usize, usize) {
    let original_len = messages.len();
    let mut dropped = 0;
    let mut repaired = 0;

    messages.retain_mut(|msg| {
        let thinking_before = msg.thinking_trace.clone();
        let reasoning_before = msg.reasoning_content.clone();
        let usage_before = msg.token_usage.clone();

        if !repair_message(msg) {
            dropped += 1;
            return false;
        }

        if msg.thinking_trace != thinking_before
            || msg.reasoning_content != reasoning_before
            || msg.token_usage != usage_before
        {
            repaired += 1;
        }

        true
    });

    if dropped > 0 || repaired > 0 {
        warn!(
            "Session message cleanup: {} dropped, {} repaired out of {} total",
            dropped, repaired, original_len
        );
    }

    (dropped, repaired)
}

#[cfg(test)]
#[path = "validator_tests.rs"]
mod tests;
