use std::collections::HashMap;

use tokio::sync::mpsc;

/// Events emitted by a running subagent, consumed by the parent agent or TUI.
#[derive(Debug, Clone)]
pub enum SubagentEvent {
    /// Subagent started.
    Started {
        subagent_id: String,
        subagent_name: String,
        task: String,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
    },
    /// Subagent made a tool call.
    ToolCall {
        subagent_id: String,
        subagent_name: String,
        tool_name: String,
        tool_id: String,
        args: HashMap<String, serde_json::Value>,
    },
    /// A subagent tool call completed.
    ToolComplete {
        subagent_id: String,
        subagent_name: String,
        tool_name: String,
        tool_id: String,
        success: bool,
    },
    /// Subagent finished.
    Finished {
        subagent_id: String,
        subagent_name: String,
        success: bool,
        result_summary: String,
        tool_call_count: usize,
        shallow_warning: Option<String>,
    },
    /// Token usage update from a subagent's LLM call.
    TokenUpdate {
        subagent_id: String,
        subagent_name: String,
        input_tokens: u64,
        output_tokens: u64,
    },
}

/// Progress callback that sends events through an mpsc channel.
///
/// Used to bridge subagent execution progress back to the TUI event loop.
pub struct ChannelProgressCallback {
    tx: mpsc::UnboundedSender<SubagentEvent>,
    /// Unique identifier for this subagent instance (disambiguates parallel subagents).
    subagent_id: String,
    /// Per-subagent cancellation token (child of parent's token).
    cancel_token: Option<tokio_util::sync::CancellationToken>,
}

impl ChannelProgressCallback {
    /// Create a new channel-based progress callback with a unique subagent ID.
    pub fn new(
        tx: mpsc::UnboundedSender<SubagentEvent>,
        subagent_id: String,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Self {
        Self {
            tx,
            subagent_id,
            cancel_token,
        }
    }
}

impl std::fmt::Debug for ChannelProgressCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelProgressCallback").finish()
    }
}

impl opendev_agents::SubagentProgressCallback for ChannelProgressCallback {
    fn on_started(&self, subagent_name: &str, task: &str) {
        let _ = self.tx.send(SubagentEvent::Started {
            subagent_id: self.subagent_id.clone(),
            subagent_name: subagent_name.to_string(),
            task: task.to_string(),
            cancel_token: self.cancel_token.clone(),
        });
    }

    fn on_tool_call(
        &self,
        subagent_name: &str,
        tool_name: &str,
        tool_id: &str,
        args: &HashMap<String, serde_json::Value>,
    ) {
        let _ = self.tx.send(SubagentEvent::ToolCall {
            subagent_id: self.subagent_id.clone(),
            subagent_name: subagent_name.to_string(),
            tool_name: tool_name.to_string(),
            tool_id: tool_id.to_string(),
            args: args.clone(),
        });
    }

    fn on_tool_complete(&self, subagent_name: &str, tool_name: &str, tool_id: &str, success: bool) {
        let _ = self.tx.send(SubagentEvent::ToolComplete {
            subagent_id: self.subagent_id.clone(),
            subagent_name: subagent_name.to_string(),
            tool_name: tool_name.to_string(),
            tool_id: tool_id.to_string(),
            success,
        });
    }

    fn on_finished(&self, _subagent_name: &str, _success: bool, _result_summary: &str) {
        // Don't emit Finished here — SpawnSubagentTool::execute() sends the
        // authoritative Finished event with correct tool_call_count and shallow_warning.
    }

    fn on_token_usage(&self, subagent_name: &str, input_tokens: u64, output_tokens: u64) {
        let _ = self.tx.send(SubagentEvent::TokenUpdate {
            subagent_id: self.subagent_id.clone(),
            subagent_name: subagent_name.to_string(),
            input_tokens,
            output_tokens,
        });
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
