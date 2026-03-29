//! Remote session bridge for Telegram.
//!
//! Provides types and a bridge that connects Telegram to a running TUI session,
//! mirroring the Claude Code Remote Control pattern: the Telegram user can
//! observe agent activity, approve/deny tool calls, send messages, and
//! cancel operations — all while the agent runs locally.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, mpsc, oneshot};

/// Events sent from the agent/TUI session to Telegram for display.
#[derive(Debug, Clone)]
pub enum RemoteEvent {
    /// Agent started processing a query.
    AgentStarted,
    /// Agent finished processing.
    AgentFinished,
    /// Agent was interrupted.
    AgentInterrupted,
    /// Streaming text chunk from the assistant.
    AgentChunk(String),
    /// Agent error.
    AgentError(String),

    /// A tool execution started.
    ToolStarted {
        tool_name: String,
        args: HashMap<String, serde_json::Value>,
    },
    /// A tool execution finished.
    ToolFinished { tool_name: String, success: bool },
    /// A tool produced its final result.
    ToolResult {
        tool_name: String,
        output: String,
        success: bool,
    },

    /// A tool requires user approval (bash command, etc.).
    ToolApprovalNeeded {
        /// Unique ID for this approval request (to match callback).
        request_id: String,
        command: String,
        working_dir: String,
    },

    /// The agent is asking the user a question.
    AskUser {
        request_id: String,
        question: String,
        options: Vec<String>,
    },

    /// A subagent started.
    SubagentStarted { subagent_name: String, task: String },
    /// A subagent finished.
    SubagentFinished {
        subagent_name: String,
        success: bool,
        result_summary: String,
    },

    /// Context usage percentage.
    ContextUsage(f64),

    /// File changes detected.
    FileChangeSummary {
        files: usize,
        additions: u64,
        deletions: u64,
    },

    /// Session title updated.
    SessionTitleUpdated(String),
}

/// Commands sent from Telegram back to the agent/TUI session.
#[derive(Debug)]
pub enum RemoteCommand {
    /// User sent a chat message.
    SendMessage(String),
    /// User approved a tool call.
    ApproveToolCall {
        request_id: String,
        /// The (possibly edited) command text.
        command: String,
    },
    /// User denied a tool call.
    DenyToolCall { request_id: String },
    /// User answered a question.
    AnswerQuestion { request_id: String, answer: String },
    /// User requested cancellation of the current operation.
    Cancel,
    /// Start a new session.
    NewSession,
    /// Resume a previous session (optionally by ID).
    ResumeSession { session_id: Option<String> },
    /// Compact context window.
    Compact,
    /// Request cost info.
    Cost,
}

/// Sender for remote events (held by the TUI/agent side).
pub type RemoteEventSender = mpsc::UnboundedSender<RemoteEvent>;
/// Receiver for remote events (held by the Telegram side).
pub type RemoteEventReceiver = mpsc::UnboundedReceiver<RemoteEvent>;

/// Sender for remote commands (held by the Telegram side).
pub type RemoteCommandSender = mpsc::UnboundedSender<RemoteCommand>;
/// Receiver for remote commands (held by the TUI/agent side).
pub type RemoteCommandReceiver = mpsc::UnboundedReceiver<RemoteCommand>;

/// Pending approval requests awaiting a Telegram callback.
struct PendingApproval {
    response_tx: oneshot::Sender<ApprovalResponse>,
}

/// Response to a pending approval.
pub enum ApprovalResponse {
    Approved { command: String },
    Denied,
}

/// Pending ask-user requests awaiting a Telegram response.
struct PendingQuestion {
    response_tx: oneshot::Sender<String>,
}

/// The remote session bridge.
///
/// Holds the channel endpoints and pending request maps. One instance lives
/// in the Telegram poller; the other ends are wired into the TUI runner.
pub struct RemoteSessionBridge {
    /// Receive events from the agent to forward to Telegram.
    pub event_rx: Mutex<RemoteEventReceiver>,
    /// Send commands from Telegram back to the agent.
    pub command_tx: RemoteCommandSender,
    /// Pending tool approval requests keyed by request_id.
    pending_approvals: Mutex<HashMap<String, PendingApproval>>,
    /// Pending ask-user requests keyed by request_id.
    pending_questions: Mutex<HashMap<String, PendingQuestion>>,
    /// Counter for generating unique request IDs.
    next_request_id: std::sync::atomic::AtomicU64,
}

impl RemoteSessionBridge {
    /// Create a new bridge and return the complementary channel endpoints.
    ///
    /// Returns `(bridge, event_tx, command_rx)`:
    /// - `bridge` is held by the Telegram poller
    /// - `event_tx` is used by the TUI runner to broadcast events
    /// - `command_rx` is polled by the TUI runner for incoming commands
    pub fn new() -> (Arc<Self>, RemoteEventSender, RemoteCommandReceiver) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let bridge = Arc::new(Self {
            event_rx: Mutex::new(event_rx),
            command_tx,
            pending_approvals: Mutex::new(HashMap::new()),
            pending_questions: Mutex::new(HashMap::new()),
            next_request_id: std::sync::atomic::AtomicU64::new(1),
        });

        (bridge, event_tx, command_rx)
    }

    /// Generate a unique request ID for pending operations.
    pub fn next_id(&self) -> String {
        let id = self
            .next_request_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("req_{id}")
    }

    /// Register a pending tool approval and return a receiver for the response.
    pub async fn register_approval(&self, request_id: &str) -> oneshot::Receiver<ApprovalResponse> {
        let (tx, rx) = oneshot::channel();
        let mut approvals = self.pending_approvals.lock().await;
        approvals.insert(request_id.to_string(), PendingApproval { response_tx: tx });
        rx
    }

    /// Resolve a pending tool approval (called when callback query arrives).
    pub async fn resolve_approval(&self, request_id: &str, response: ApprovalResponse) -> bool {
        let mut approvals = self.pending_approvals.lock().await;
        if let Some(pending) = approvals.remove(request_id) {
            let _ = pending.response_tx.send(response);
            true
        } else {
            false
        }
    }

    /// Register a pending question and return a receiver for the answer.
    pub async fn register_question(&self, request_id: &str) -> oneshot::Receiver<String> {
        let (tx, rx) = oneshot::channel();
        let mut questions = self.pending_questions.lock().await;
        questions.insert(request_id.to_string(), PendingQuestion { response_tx: tx });
        rx
    }

    /// Resolve a pending question (called when user replies).
    pub async fn resolve_question(&self, request_id: &str, answer: String) -> bool {
        let mut questions = self.pending_questions.lock().await;
        if let Some(pending) = questions.remove(request_id) {
            let _ = pending.response_tx.send(answer);
            true
        } else {
            false
        }
    }

    /// Send a command to the agent/TUI.
    pub fn send_command(&self, cmd: RemoteCommand) {
        let _ = self.command_tx.send(cmd);
    }
}
