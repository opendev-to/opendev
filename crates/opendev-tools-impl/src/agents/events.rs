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

    // -- Background agent events (run_in_background) --
    /// A background agent was spawned (returns task_id immediately).
    BackgroundSpawned {
        task_id: String,
        agent_type: String,
        query: String,
        description: String,
        session_id: String,
        interrupt_token: opendev_runtime::InterruptToken,
    },
    /// A background agent completed (success or failure).
    BackgroundCompleted {
        task_id: String,
        success: bool,
        result_summary: String,
        full_result: String,
        cost_usd: f64,
        tool_call_count: usize,
    },
    /// Progress update from a background agent.
    BackgroundProgress {
        task_id: String,
        tool_name: String,
        tool_count: usize,
    },
    /// Activity line from a background agent.
    BackgroundActivity { task_id: String, line: String },
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

/// Progress callback for background agents spawned with `run_in_background`.
///
/// Emits `BackgroundProgress` and `BackgroundActivity` events.
/// All other methods (chunk, reasoning, context_usage) are no-ops to
/// prevent background tool events from leaking into the foreground display.
pub struct BackgroundProgressCallback {
    tx: mpsc::UnboundedSender<SubagentEvent>,
    task_id: String,
    tool_count: std::sync::atomic::AtomicUsize,
}

impl BackgroundProgressCallback {
    pub fn new(tx: mpsc::UnboundedSender<SubagentEvent>, task_id: String) -> Self {
        Self {
            tx,
            task_id,
            tool_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl std::fmt::Debug for BackgroundProgressCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundProgressCallback")
            .field("task_id", &self.task_id)
            .finish()
    }
}

impl opendev_agents::SubagentProgressCallback for BackgroundProgressCallback {
    fn on_started(&self, _subagent_name: &str, _task: &str) {}

    fn on_tool_call(
        &self,
        _subagent_name: &str,
        tool_name: &str,
        _tool_id: &str,
        _args: &HashMap<String, serde_json::Value>,
    ) {
        let count = self
            .tool_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        let _ = self.tx.send(SubagentEvent::BackgroundProgress {
            task_id: self.task_id.clone(),
            tool_name: tool_name.to_string(),
            tool_count: count,
        });
        let _ = self.tx.send(SubagentEvent::BackgroundActivity {
            task_id: self.task_id.clone(),
            line: format!("\u{25b8} {tool_name}"),
        });
    }

    fn on_tool_complete(
        &self,
        _subagent_name: &str,
        tool_name: &str,
        _tool_id: &str,
        success: bool,
    ) {
        let icon = if success { "\u{2713}" } else { "\u{2717}" };
        let _ = self.tx.send(SubagentEvent::BackgroundActivity {
            task_id: self.task_id.clone(),
            line: format!("  \u{23bf} {icon} {tool_name}"),
        });
    }

    fn on_finished(&self, _subagent_name: &str, _success: bool, _result_summary: &str) {
        // BackgroundCompleted event sent by spawn_background(), not here
    }

    fn on_token_usage(&self, _subagent_name: &str, _input_tokens: u64, _output_tokens: u64) {}
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
