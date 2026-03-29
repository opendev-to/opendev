//! Shared frontend event types consumed by both TUI and Web UI.
//!
//! This module defines the canonical set of events that frontends render.
//! The Web UI serializes these to JSON over WebSocket; the TUI will
//! consume them directly once the incremental convergence from `AppEvent`
//! is complete.
//!
//! All public types derive `ts_rs::TS` so TypeScript definitions can be
//! auto-generated with `cargo test -p opendev-models export_frontend_types`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

// ─── Top-Level Event Enum ───────────────────────────────────────────────────

/// A frontend event that both the TUI and Web UI can render.
///
/// Serialized as a tagged union: `{"type": "MessageChunk", "data": {...}}`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
#[ts(export)]
pub enum FrontendEvent {
    // ── Message Lifecycle ────────────────────────────────────────────────
    MessageStart(MessageStartPayload),
    MessageChunk(MessageChunkPayload),
    MessageComplete(MessageCompletePayload),

    // ── Tool Events ─────────────────────────────────────────────────────
    ToolCall(ToolCallPayload),
    ToolResult(ToolResultPayload),

    // ── Thinking / Reasoning ────────────────────────────────────────────
    ThinkingBlock(ThinkingBlockPayload),

    // ── Subagent Events ─────────────────────────────────────────────────
    SubagentStarted(SubagentStartedPayload),
    SubagentCompleted(SubagentCompletedPayload),
    NestedToolCall(NestedToolCallPayload),
    NestedToolResult(NestedToolResultPayload),

    // ── Status & Progress ───────────────────────────────────────────────
    StatusUpdate(StatusUpdatePayload),
    Progress(ProgressPayload),

    // ── Approval / Ask-User / Plan ──────────────────────────────────────
    ApprovalRequired(ApprovalRequiredPayload),
    AskUserRequired(AskUserRequiredPayload),
    PlanApprovalRequired(PlanApprovalRequiredPayload),

    // ── Session Activity ────────────────────────────────────────────────
    SessionActivity(SessionActivityPayload),

    // ── Errors ──────────────────────────────────────────────────────────
    Error(ErrorPayload),
}

// ─── Payload Structs ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MessageStartPayload {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MessageChunkPayload {
    pub session_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MessageCompletePayload {
    pub session_id: String,
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolCallPayload {
    pub session_id: String,
    pub tool_id: String,
    pub tool_name: String,
    #[ts(type = "Record<string, any>")]
    pub arguments: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolResultPayload {
    pub session_id: String,
    pub tool_id: String,
    pub tool_name: String,
    pub output: String,
    pub success: bool,
    /// Optional todo state included when tool is a todo-related tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub todos: Option<Vec<TodoItemPayload>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TodoItemPayload {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_form: Option<String>,
    #[serde(default)]
    pub children: Vec<TodoChildPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TodoChildPayload {
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThinkingBlockPayload {
    pub session_id: String,
    pub content: String,
    #[serde(default)]
    pub block_start: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubagentStartedPayload {
    pub subagent_id: String,
    pub agent_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubagentCompletedPayload {
    pub subagent_id: String,
    pub success: bool,
    pub result_summary: String,
    pub tool_call_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shallow_warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NestedToolCallPayload {
    pub subagent_id: String,
    pub tool_name: String,
    pub tool_id: String,
    pub arguments: HashMap<String, serde_json::Value>,
    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NestedToolResultPayload {
    pub subagent_id: String,
    pub tool_name: String,
    pub tool_id: String,
    pub success: bool,
    #[serde(default = "default_depth")]
    pub depth: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatusUpdatePayload {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_usage_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autonomy_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_changes: Option<FileChangesPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub todos: Option<Vec<TodoItemPayload>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FileChangesPayload {
    pub files: usize,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProgressPayload {
    pub session_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApprovalRequiredPayload {
    pub id: String,
    pub tool_name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AskUserRequiredPayload {
    pub request_id: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlanApprovalRequiredPayload {
    pub request_id: String,
    pub plan_content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionActivityPayload {
    pub session_id: String,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ErrorPayload {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

// ─── TypeScript Export Test ─────────────────────────────────────────────────

#[cfg(test)]
#[path = "frontend_event_tests.rs"]
mod tests;
