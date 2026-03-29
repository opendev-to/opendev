//! WebSocket message type registry.
//!
//! Provides a canonical enum of all WebSocket message types used between the
//! server and the React frontend. The `Display` / `Serialize` implementations
//! produce the exact same JSON strings the frontend expects.

use serde::{Deserialize, Serialize};
use std::fmt;

/// All known WebSocket message types, matching the Python `WSMessageType` enum.
///
/// Variants are grouped into server-to-client and client-to-server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WsMessageType {
    // ── Server -> Client ────────────────────────────────────────────
    /// A tool call is being made by the agent.
    #[serde(rename = "tool_call")]
    ToolCall,
    /// Result of a tool call.
    #[serde(rename = "tool_result")]
    ToolResult,
    /// An approval is required before executing a tool.
    #[serde(rename = "approval_required")]
    ApprovalRequired,
    /// An approval request has been resolved.
    #[serde(rename = "approval_resolved")]
    ApprovalResolved,
    /// The agent needs user input via ask-user.
    #[serde(rename = "ask_user_required")]
    AskUserRequired,
    /// An ask-user request has been resolved.
    #[serde(rename = "ask_user_resolved")]
    AskUserResolved,
    /// Plan content from the planning agent.
    #[serde(rename = "plan_content")]
    PlanContent,
    /// A plan requires user approval.
    #[serde(rename = "plan_approval_required")]
    PlanApprovalRequired,
    /// A plan approval request has been resolved.
    #[serde(rename = "plan_approval_resolved")]
    PlanApprovalResolved,
    /// Status update (mode, autonomy, thinking, etc.).
    #[serde(rename = "status_update")]
    StatusUpdate,
    /// The agent has completed its task.
    #[serde(rename = "task_completed")]
    TaskCompleted,
    /// A subagent has started execution.
    #[serde(rename = "subagent_start")]
    SubagentStart,
    /// A subagent has completed execution.
    #[serde(rename = "subagent_complete")]
    SubagentComplete,
    /// Parallel agent execution started.
    #[serde(rename = "parallel_agents_start")]
    ParallelAgentsStart,
    /// Parallel agent execution completed.
    #[serde(rename = "parallel_agents_done")]
    ParallelAgentsDone,
    /// A thinking/reasoning block from the model.
    #[serde(rename = "thinking_block")]
    ThinkingBlock,
    /// Progress update during a long-running operation.
    #[serde(rename = "progress")]
    Progress,
    /// A tool call within a nested/subagent context.
    #[serde(rename = "nested_tool_call")]
    NestedToolCall,
    /// Result of a nested tool call.
    #[serde(rename = "nested_tool_result")]
    NestedToolResult,
    /// Streaming message chunk.
    #[serde(rename = "message_chunk")]
    MessageChunk,
    /// Start of a new assistant message.
    #[serde(rename = "message_start")]
    MessageStart,
    /// Completion of an assistant message.
    #[serde(rename = "message_complete")]
    MessageComplete,
    /// Session activity event.
    #[serde(rename = "session_activity")]
    SessionActivity,
    /// User message (echoed back or broadcast).
    #[serde(rename = "user_message")]
    UserMessage,
    /// MCP server status changed.
    #[serde(rename = "mcp:status_changed")]
    McpStatusChanged,
    /// MCP servers list updated.
    #[serde(rename = "mcp:servers_updated")]
    McpServersUpdated,
    /// Error message.
    #[serde(rename = "error")]
    Error,
    /// Pong response to a ping.
    #[serde(rename = "pong")]
    Pong,

    // ── Client -> Server ────────────────────────────────────────────
    /// Query / user message from the client.
    #[serde(rename = "query")]
    Query,
    /// Approval response from the client.
    #[serde(rename = "approve")]
    Approve,
    /// Ask-user response from the client.
    #[serde(rename = "ask_user_response")]
    AskUserResponse,
    /// Plan approval response from the client.
    #[serde(rename = "plan_approval_response")]
    PlanApprovalResponse,
    /// Ping keepalive.
    #[serde(rename = "ping")]
    Ping,
    /// Interrupt request.
    #[serde(rename = "interrupt")]
    Interrupt,
}

impl WsMessageType {
    /// Get the string representation matching the wire format.
    pub fn as_str(&self) -> &'static str {
        match self {
            // Server -> Client
            Self::ToolCall => "tool_call",
            Self::ToolResult => "tool_result",
            Self::ApprovalRequired => "approval_required",
            Self::ApprovalResolved => "approval_resolved",
            Self::AskUserRequired => "ask_user_required",
            Self::AskUserResolved => "ask_user_resolved",
            Self::PlanContent => "plan_content",
            Self::PlanApprovalRequired => "plan_approval_required",
            Self::PlanApprovalResolved => "plan_approval_resolved",
            Self::StatusUpdate => "status_update",
            Self::TaskCompleted => "task_completed",
            Self::SubagentStart => "subagent_start",
            Self::SubagentComplete => "subagent_complete",
            Self::ParallelAgentsStart => "parallel_agents_start",
            Self::ParallelAgentsDone => "parallel_agents_done",
            Self::ThinkingBlock => "thinking_block",
            Self::Progress => "progress",
            Self::NestedToolCall => "nested_tool_call",
            Self::NestedToolResult => "nested_tool_result",
            Self::MessageChunk => "message_chunk",
            Self::MessageStart => "message_start",
            Self::MessageComplete => "message_complete",
            Self::SessionActivity => "session_activity",
            Self::UserMessage => "user_message",
            Self::McpStatusChanged => "mcp:status_changed",
            Self::McpServersUpdated => "mcp:servers_updated",
            Self::Error => "error",
            Self::Pong => "pong",
            // Client -> Server
            Self::Query => "query",
            Self::Approve => "approve",
            Self::AskUserResponse => "ask_user_response",
            Self::PlanApprovalResponse => "plan_approval_response",
            Self::Ping => "ping",
            Self::Interrupt => "interrupt",
        }
    }

    /// Parse a message type string into the enum.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "tool_call" => Some(Self::ToolCall),
            "tool_result" => Some(Self::ToolResult),
            "approval_required" => Some(Self::ApprovalRequired),
            "approval_resolved" => Some(Self::ApprovalResolved),
            "ask_user_required" => Some(Self::AskUserRequired),
            "ask_user_resolved" => Some(Self::AskUserResolved),
            "plan_content" => Some(Self::PlanContent),
            "plan_approval_required" => Some(Self::PlanApprovalRequired),
            "plan_approval_resolved" => Some(Self::PlanApprovalResolved),
            "status_update" => Some(Self::StatusUpdate),
            "task_completed" => Some(Self::TaskCompleted),
            "subagent_start" => Some(Self::SubagentStart),
            "subagent_complete" => Some(Self::SubagentComplete),
            "parallel_agents_start" => Some(Self::ParallelAgentsStart),
            "parallel_agents_done" => Some(Self::ParallelAgentsDone),
            "thinking_block" => Some(Self::ThinkingBlock),
            "progress" => Some(Self::Progress),
            "nested_tool_call" => Some(Self::NestedToolCall),
            "nested_tool_result" => Some(Self::NestedToolResult),
            "message_chunk" => Some(Self::MessageChunk),
            "message_start" => Some(Self::MessageStart),
            "message_complete" => Some(Self::MessageComplete),
            "session_activity" => Some(Self::SessionActivity),
            "user_message" => Some(Self::UserMessage),
            "mcp:status_changed" => Some(Self::McpStatusChanged),
            "mcp:servers_updated" => Some(Self::McpServersUpdated),
            "error" => Some(Self::Error),
            "pong" => Some(Self::Pong),
            "query" => Some(Self::Query),
            "approve" => Some(Self::Approve),
            "ask_user_response" => Some(Self::AskUserResponse),
            "plan_approval_response" => Some(Self::PlanApprovalResponse),
            "ping" => Some(Self::Ping),
            "interrupt" => Some(Self::Interrupt),
            _ => None,
        }
    }
}

impl fmt::Display for WsMessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Construct a standard WebSocket message envelope.
pub fn ws_message(msg_type: WsMessageType, data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": msg_type.as_str(),
        "data": data,
    })
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
