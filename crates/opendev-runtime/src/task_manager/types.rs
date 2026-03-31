//! Types for the agent task state machine.

use serde::{Deserialize, Serialize};

/// Lifecycle state of an agent task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
    Failed,
    Killed,
}

impl TaskState {
    /// Whether the task is in a terminal state (no further transitions).
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Killed)
    }
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Killed => write!(f, "killed"),
        }
    }
}

/// Rich tool activity entry for progress display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolActivity {
    pub tool_name: String,
    pub description: String,
    pub is_search: bool,
    pub is_read: bool,
    pub started_at_ms: u64,
    pub finished: bool,
    pub success: bool,
}

/// A message queued for delivery to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMessage {
    pub from_agent: String,
    pub content: String,
    pub timestamp_ms: u64,
}

/// Maximum number of recent activities kept per task.
pub const MAX_RECENT_ACTIVITIES: usize = 5;

/// Maximum number of activity log lines kept per task.
pub const MAX_ACTIVITY_LOG: usize = 200;

/// Default grace period before eviction (milliseconds).
pub const EVICT_GRACE_MS: u64 = 5_000;

/// Full state of a single agent task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub agent_type: String,
    /// Short (3-8 word) label for UI display.
    pub description: String,
    /// Full task prompt.
    pub query: String,
    pub session_id: String,
    pub state: TaskState,
    /// Whether the task is running in the background.
    pub is_backgrounded: bool,
    /// True if spawned with `run_in_background` (vs Ctrl+B'd).
    pub was_async_spawn: bool,
    pub created_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub completed_at_ms: Option<u64>,
    pub tool_call_count: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub current_tool: Option<String>,
    pub result_summary: Option<String>,
    pub full_result: Option<String>,
    pub parent_task_id: Option<String>,
    pub team_id: Option<String>,
    /// Rolling activity log (plain text lines, capped at [`MAX_ACTIVITY_LOG`]).
    pub activity_log: Vec<String>,
    /// Last N rich tool activities for progress display.
    pub recent_activities: Vec<ToolActivity>,
    /// The single most recent activity.
    pub last_activity: Option<ToolActivity>,
    /// Messages queued for delivery to this agent.
    pub pending_messages: Vec<PendingMessage>,
    /// Whether the completion notification has been sent (prevents duplicates).
    pub notified: bool,
    /// Timestamp (epoch ms) after which this task can be evicted from state.
    pub evict_after_ms: Option<u64>,
    /// When true, UI is viewing this task — blocks eviction.
    pub retain: bool,
}

impl Default for TaskInfo {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            agent_type: String::new(),
            description: String::new(),
            query: String::new(),
            session_id: String::new(),
            state: TaskState::Pending,
            is_backgrounded: false,
            was_async_spawn: false,
            created_at_ms: 0,
            started_at_ms: None,
            completed_at_ms: None,
            tool_call_count: 0,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            current_tool: None,
            result_summary: None,
            full_result: None,
            parent_task_id: None,
            team_id: None,
            activity_log: Vec::new(),
            recent_activities: Vec::new(),
            last_activity: None,
            pending_messages: Vec::new(),
            notified: false,
            evict_after_ms: None,
            retain: false,
        }
    }
}

/// Events published by the task manager on state transitions.
#[derive(Debug, Clone)]
pub enum TaskManagerEvent {
    StateChanged {
        task_id: String,
        old: TaskState,
        new: TaskState,
    },
    Progress {
        task_id: String,
        tool_name: String,
        tool_count: usize,
    },
    MessageReceived {
        task_id: String,
        from: String,
    },
}
