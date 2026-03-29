//! Type definitions for the subagent manager.
//!
//! Contains the `SubagentType` enum, progress callback traits,
//! the `SubagentEventBridge`, and the `SubagentRunResult`.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::traits::AgentResult;

/// Well-known subagent types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubagentType {
    CodeExplorer,
    Planner,
    General,
    Build,
    AskUser,
    Custom,
}

impl SubagentType {
    /// Parse a subagent type from a name string.
    pub fn from_name(name: &str) -> Self {
        match name {
            "Explore" | "Code-Explorer" | "code_explorer" => Self::CodeExplorer,
            "Planner" | "planner" => Self::Planner,
            "General" | "general" => Self::General,
            "Build" | "build" => Self::Build,
            "ask-user" | "ask_user" => Self::AskUser,
            _ => Self::Custom,
        }
    }

    /// Get the canonical name for this type.
    pub fn canonical_name(&self) -> &'static str {
        match self {
            Self::CodeExplorer => "Explore",
            Self::Planner => "Planner",
            Self::General => "General",
            Self::Build => "Build",
            Self::AskUser => "ask-user",
            Self::Custom => "custom",
        }
    }
}

/// Progress callback for subagent lifecycle events.
///
/// The parent TUI or caller can implement this to receive real-time
/// updates about the subagent's execution progress.
pub trait SubagentProgressCallback: Send + Sync {
    /// Called when the subagent starts executing.
    fn on_started(&self, subagent_name: &str, task: &str);

    /// Called when the subagent invokes a tool.
    fn on_tool_call(
        &self,
        subagent_name: &str,
        tool_name: &str,
        tool_id: &str,
        args: &HashMap<String, serde_json::Value>,
    );

    /// Called when a subagent tool call completes.
    fn on_tool_complete(&self, subagent_name: &str, tool_name: &str, tool_id: &str, success: bool);

    /// Called when the subagent finishes (with or without error).
    fn on_finished(&self, subagent_name: &str, success: bool, result_summary: &str);

    /// Called when token usage is reported from an LLM call.
    fn on_token_usage(&self, _subagent_name: &str, _input_tokens: u64, _output_tokens: u64) {}
}

/// A no-op progress callback for when the caller doesn't need progress updates.
#[derive(Debug)]
pub struct NoopProgressCallback;

impl SubagentProgressCallback for NoopProgressCallback {
    fn on_started(&self, _name: &str, _task: &str) {}
    fn on_tool_call(
        &self,
        _name: &str,
        _tool: &str,
        _id: &str,
        _args: &HashMap<String, serde_json::Value>,
    ) {
    }
    fn on_tool_complete(&self, _name: &str, _tool: &str, _id: &str, _success: bool) {}
    fn on_finished(&self, _name: &str, _success: bool, _summary: &str) {}
}

/// Bridge that forwards `AgentEventCallback` events from the subagent's
/// react loop to the parent's `SubagentProgressCallback`.
///
/// This makes subagent tool calls visible to the TUI in real-time.
pub struct SubagentEventBridge {
    subagent_name: String,
    progress: Arc<dyn SubagentProgressCallback>,
}

impl SubagentEventBridge {
    /// Create a new bridge for a given subagent.
    pub fn new(subagent_name: String, progress: Arc<dyn SubagentProgressCallback>) -> Self {
        Self {
            subagent_name,
            progress,
        }
    }
}

impl crate::traits::AgentEventCallback for SubagentEventBridge {
    fn on_tool_started(
        &self,
        tool_id: &str,
        tool_name: &str,
        args: &std::collections::HashMap<String, serde_json::Value>,
    ) {
        debug!(
            subagent = %self.subagent_name,
            tool_name = %tool_name,
            tool_id = %tool_id,
            "SubagentEventBridge: forwarding tool_started → on_tool_call"
        );
        self.progress
            .on_tool_call(&self.subagent_name, tool_name, tool_id, args);
    }

    fn on_tool_finished(&self, tool_id: &str, success: bool) {
        debug!(
            subagent = %self.subagent_name,
            tool_id = %tool_id,
            success = %success,
            "SubagentEventBridge: forwarding tool_finished → on_tool_complete"
        );
        self.progress
            .on_tool_complete(&self.subagent_name, "", tool_id, success);
    }

    fn on_agent_chunk(&self, _text: &str) {}

    fn on_token_usage(&self, input_tokens: u64, output_tokens: u64) {
        self.progress
            .on_token_usage(&self.subagent_name, input_tokens, output_tokens);
    }
}

/// Result of spawning a subagent, containing the result and diagnostic info.
#[derive(Debug, Clone)]
pub struct SubagentRunResult {
    /// The agent result from the subagent's ReAct loop.
    pub agent_result: AgentResult,
    /// Number of tool calls the subagent made.
    pub tool_call_count: usize,
    /// Whether the shallow subagent warning applies.
    pub shallow_warning: Option<String>,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
