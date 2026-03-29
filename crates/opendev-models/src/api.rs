//! Shared API response models used by Web routes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::message::ToolCall;

/// Serialization view of a ToolCall for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    pub id: String,
    pub name: String,
    pub parameters: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nested_tool_calls: Option<Vec<ToolCallResponse>>,
}

/// Response model for a chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResponse {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_trace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

/// Session information model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub id: String,
    pub working_dir: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub has_session_model: bool,
}

/// Recursively convert a ToolCall to ToolCallResponse.
///
/// Handles nested calls and coerces non-string results to JSON strings.
pub fn tool_call_to_response(tc: &ToolCall) -> ToolCallResponse {
    let nested = if tc.nested_tool_calls.is_empty() {
        None
    } else {
        Some(
            tc.nested_tool_calls
                .iter()
                .map(tool_call_to_response)
                .collect(),
        )
    };

    let result = tc.result.as_ref().map(|r| {
        if let serde_json::Value::String(s) = r {
            s.clone()
        } else {
            serde_json::to_string(r).unwrap_or_else(|_| r.to_string())
        }
    });

    ToolCallResponse {
        id: tc.id.clone(),
        name: tc.name.clone(),
        parameters: tc.parameters.clone(),
        result,
        error: tc.error.clone(),
        result_summary: tc.result_summary.clone(),
        approved: Some(tc.approved),
        nested_tool_calls: nested,
    }
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
