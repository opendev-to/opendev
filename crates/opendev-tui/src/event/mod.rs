//! Event types for the TUI application.
//!
//! Bridges crossterm terminal events with application-level events
//! (agent messages, tool execution updates, etc.).
//!
//! This module is split into focused sub-modules:
//! - [`handler`] — Crossterm event reader with scroll debouncing
//! - [`recorder`] — Event recording/replay for debugging

mod handler;
mod recorder;

pub use handler::EventHandler;
pub use recorder::{EventRecorder, RecordedEvent, load_recorded_events};

use crossterm::event::{Event as CrosstermEvent, KeyEvent};
use opendev_models::message::ChatMessage;
use opendev_runtime::InterruptToken;

/// Application-level events consumed by the main event loop.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AppEvent {
    /// Raw terminal event from crossterm.
    Terminal(CrosstermEvent),
    /// Key press (extracted from terminal event for convenience).
    Key(KeyEvent),
    /// Terminal resize.
    Resize(u16, u16),
    /// Mouse-wheel scroll up.
    ScrollUp,
    /// Mouse-wheel scroll down.
    ScrollDown,
    /// Mouse button pressed at (col, row).
    MouseDown { col: u16, row: u16 },
    /// Mouse dragged to (col, row) while button held.
    MouseDrag { col: u16, row: u16 },
    /// Mouse button released at (col, row).
    MouseUp { col: u16, row: u16 },
    /// Terminal regained focus (user switched back to this tab/window).
    FocusGained,
    /// Tick for periodic UI updates (spinner animation, etc.).
    Tick,

    // -- Agent events --
    /// Assistant started generating a response.
    AgentStarted,
    /// Streaming text chunk from the assistant.
    AgentChunk(String),
    /// Complete assistant message received.
    AgentMessage(ChatMessage),
    /// Agent finished the current turn.
    AgentFinished,
    /// Agent encountered an error.
    AgentError(String),

    // -- Tool events --
    /// A tool execution started.
    ToolStarted {
        tool_id: String,
        tool_name: String,
        args: std::collections::HashMap<String, serde_json::Value>,
    },
    /// A tool produced output.
    ToolOutput { tool_id: String, output: String },
    /// A tool produced its final result.
    ToolResult {
        tool_id: String,
        tool_name: String,
        output: String,
        success: bool,
        args: std::collections::HashMap<String, serde_json::Value>,
    },
    /// A tool execution completed.
    ToolFinished { tool_id: String, success: bool },
    /// Tool requires user approval (legacy, no channel — kept for recording compatibility).
    ToolApprovalRequired {
        tool_id: String,
        tool_name: String,
        description: String,
    },

    /// Tool approval request with bidirectional channel.
    ToolApprovalRequested {
        command: String,
        working_dir: String,
        response_tx: tokio::sync::oneshot::Sender<opendev_runtime::ToolApprovalDecision>,
    },

    /// Ask-user request with bidirectional channel.
    AskUserRequested {
        question: String,
        options: Vec<String>,
        default: Option<String>,
        response_tx: tokio::sync::oneshot::Sender<String>,
    },

    // -- Subagent events --
    /// A subagent started executing.
    SubagentStarted {
        subagent_id: String,
        subagent_name: String,
        task: String,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
    },
    /// A subagent made a tool call (for nested display).
    SubagentToolCall {
        subagent_id: String,
        subagent_name: String,
        tool_name: String,
        tool_id: String,
        args: std::collections::HashMap<String, serde_json::Value>,
    },
    /// A subagent tool call completed.
    SubagentToolComplete {
        subagent_id: String,
        subagent_name: String,
        tool_name: String,
        tool_id: String,
        success: bool,
    },
    /// A subagent finished its task.
    SubagentFinished {
        subagent_id: String,
        subagent_name: String,
        success: bool,
        result_summary: String,
        tool_call_count: usize,
        shallow_warning: Option<String>,
    },
    /// Token usage update from a subagent's LLM call.
    SubagentTokenUpdate {
        subagent_id: String,
        subagent_name: String,
        input_tokens: u64,
        output_tokens: u64,
    },
    // -- Reasoning events --
    /// Native reasoning content from LLM response (inline thinking).
    ReasoningContent(String),
    /// A new reasoning/thinking block started (separator between interleaved blocks).
    ReasoningBlockStart,

    // -- Task progress events --
    /// Agent started working on a task (shows progress bar).
    TaskProgressStarted { description: String },
    /// Agent finished the current task (hides progress bar).
    TaskProgressFinished,

    // -- Budget events --
    /// Session cost budget has been exhausted. The agent loop should pause.
    BudgetExhausted { cost_usd: f64, budget_usd: f64 },

    // -- File change events --
    /// File change summary after a query completes.
    FileChangeSummary {
        files: usize,
        additions: u64,
        deletions: u64,
    },

    // -- Context events --
    /// Context window usage percentage updated (0.0–100.0).
    ContextUsage(f64),

    // -- Compaction events --
    /// Manual compaction started (shows compaction spinner).
    CompactionStarted,
    /// Manual compaction finished (hides compaction spinner, shows result).
    CompactionFinished { success: bool, message: String },

    // -- Plan events --
    /// Plan approval request arrived from the PresentPlanTool.
    /// Contains the plan content to display and the oneshot sender for the decision.
    PlanApprovalRequested {
        plan_content: String,
        response_tx: tokio::sync::oneshot::Sender<opendev_runtime::PlanDecision>,
    },

    // -- UI events --
    /// User submitted a message.
    UserSubmit(String),
    /// User requested interrupt (Escape).
    Interrupt,
    /// Set the interrupt token for the current query (sent by agent backend).
    SetInterruptToken(InterruptToken),
    /// Agent run was interrupted (sent by agent backend after cancellation).
    AgentInterrupted,
    /// Mode changed (normal/plan).
    ModeChanged(String),
    /// Kill a background task by ID.
    KillTask(String),

    // -- Background agent events --
    /// An agent was moved to the background via Ctrl+B.
    AgentBackgrounded {
        task_id: String,
        query_summary: String,
    },
    /// A background agent completed its work.
    BackgroundAgentCompleted {
        task_id: String,
        success: bool,
        result_summary: String,
        full_result: String,
        cost_usd: f64,
        tool_call_count: usize,
    },
    /// Progress update from a background agent.
    BackgroundAgentProgress {
        task_id: String,
        tool_name: String,
        tool_count: usize,
    },
    /// A background agent was killed.
    BackgroundAgentKilled { task_id: String },
    /// LLM-generated nudge message after backgrounding agents.
    BackgroundNudge { content: String },
    /// Activity line from a background agent (tool call, reasoning, etc.).
    BackgroundAgentActivity { task_id: String, line: String },
    /// Register a background agent task with its interrupt token (sent from tui_runner).
    SetBackgroundAgentToken {
        task_id: String,
        query: String,
        session_id: String,
        interrupt_token: InterruptToken,
    },

    // -- Undo/Redo events --
    /// Snapshot was taken (stores tree hash for undo stack).
    SnapshotTaken { hash: String },
    /// Undo result from the runtime.
    UndoResult { success: bool, message: String },
    /// Redo result from the runtime.
    RedoResult { success: bool, message: String },
    /// Share result from the runtime.
    ShareResult { path: String },
    /// File watcher detected changes.
    FileChanged { paths: Vec<String> },

    /// Session title was auto-detected by the topic detector.
    SessionTitleUpdated(String),

    /// Session cost updated (cumulative USD from cost tracker).
    CostUpdate(f64),

    /// Quit the application.
    Quit,
}

#[cfg(test)]
mod tests;
