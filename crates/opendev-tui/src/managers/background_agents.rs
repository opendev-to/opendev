//! Background agent task manager for tracking agent runs moved to background via Ctrl+B.

use std::collections::HashMap;
use std::time::Instant;

use opendev_runtime::InterruptToken;

/// State of a background agent task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundAgentState {
    Running,
    Completed,
    Failed,
    Killed,
}

impl std::fmt::Display for BackgroundAgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Killed => write!(f, "killed"),
        }
    }
}

/// A single background agent task.
#[derive(Debug)]
pub struct BackgroundAgentTask {
    /// Short hex task ID.
    pub task_id: String,
    /// Original user query.
    pub query: String,
    /// Forked session ID.
    pub session_id: String,
    /// When the task was started.
    pub started_at: Instant,
    /// Current state.
    pub state: BackgroundAgentState,
    /// Interrupt token for killing (request() cancels inner CancellationToken too).
    pub interrupt_token: InterruptToken,
    /// Result summary (set on completion).
    pub result_summary: Option<String>,
    /// Number of tool calls made.
    pub tool_call_count: usize,
    /// Total cost in USD.
    pub cost_usd: f64,
    /// Current tool being executed (for progress display).
    pub current_tool: Option<String>,
    /// Rolling log of tool call activity (for panel display).
    pub activity_log: Vec<String>,
    /// Number of pending spawn_subagent calls awaiting SubagentStarted events.
    pub pending_spawn_count: usize,
    /// Whether this task should be hidden from the task watcher grid.
    /// Set when a parent task is killed as cascade from last subagent cancellation.
    pub hidden: bool,
}

impl BackgroundAgentTask {
    /// Runtime in seconds.
    pub fn runtime_seconds(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    /// Whether the task is still running.
    pub fn is_running(&self) -> bool {
        self.state == BackgroundAgentState::Running
    }
}

/// Manages background agent tasks.
#[derive(Debug)]
pub struct BackgroundAgentManager {
    tasks: HashMap<String, BackgroundAgentTask>,
    /// Maximum concurrent background agent tasks.
    pub max_concurrent: usize,
}

impl BackgroundAgentManager {
    /// Create a new manager with the default concurrency limit.
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            max_concurrent: 3,
        }
    }

    /// Add a new background agent task.
    pub fn add_task(
        &mut self,
        task_id: String,
        query: String,
        session_id: String,
        interrupt_token: InterruptToken,
    ) {
        self.tasks.insert(
            task_id.clone(),
            BackgroundAgentTask {
                task_id,
                query,
                session_id,
                started_at: Instant::now(),
                state: BackgroundAgentState::Running,
                interrupt_token,
                result_summary: None,
                tool_call_count: 0,
                cost_usd: 0.0,
                current_tool: None,
                activity_log: Vec::new(),
                pending_spawn_count: 0,
                hidden: false,
            },
        );
    }

    /// Mark a task as completed.
    pub fn mark_completed(
        &mut self,
        task_id: &str,
        success: bool,
        result_summary: String,
        tool_call_count: usize,
        cost_usd: f64,
    ) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            if task.state != BackgroundAgentState::Killed {
                task.state = if success {
                    BackgroundAgentState::Completed
                } else {
                    BackgroundAgentState::Failed
                };
            }
            task.result_summary = Some(result_summary);
            task.tool_call_count = tool_call_count;
            task.cost_usd = cost_usd;
        }
    }

    /// Update progress for a running task.
    pub fn update_progress(&mut self, task_id: &str, tool_name: String, tool_count: usize) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.current_tool = Some(tool_name);
            task.tool_call_count = tool_count;
        }
    }

    /// Increment pending spawn_subagent count for a task.
    pub fn increment_pending_spawn(&mut self, task_id: &str) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.pending_spawn_count += 1;
        }
    }

    /// Decrement pending spawn_subagent count for a task.
    pub fn decrement_pending_spawn(&mut self, task_id: &str) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.pending_spawn_count = task.pending_spawn_count.saturating_sub(1);
        }
    }

    /// Append an activity line to a task's log (capped at 50 entries).
    /// Reasoning lines (\u{27e1} prefix) are coalesced — the last one is replaced
    /// instead of pushing a new entry, preventing word-per-line spam from streaming tokens.
    pub fn push_activity(&mut self, task_id: &str, line: String) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            if line.starts_with('\u{27e1}')
                && task
                    .activity_log
                    .last()
                    .is_some_and(|l| l.starts_with('\u{27e1}'))
            {
                *task.activity_log.last_mut().unwrap() = line;
                return;
            }
            if task.activity_log.len() >= 50 {
                task.activity_log.remove(0);
            }
            task.activity_log.push(line);
        }
    }

    /// Hide a task from the task watcher grid (still tracked for event processing).
    pub fn hide_task(&mut self, task_id: &str) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.hidden = true;
        }
    }

    /// Kill a running task.
    pub fn kill_task(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.get_mut(task_id)
            && task.is_running()
        {
            task.interrupt_token.request();
            task.state = BackgroundAgentState::Killed;
            return true;
        }
        false
    }

    /// Get a task by ID.
    pub fn get_task(&self, task_id: &str) -> Option<&BackgroundAgentTask> {
        self.tasks.get(task_id)
    }

    /// Get all tasks sorted by start time (newest first).
    pub fn all_tasks(&self) -> Vec<&BackgroundAgentTask> {
        let mut tasks: Vec<&BackgroundAgentTask> = self.tasks.values().collect();
        tasks.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        tasks
    }

    /// Get the number of running tasks.
    pub fn running_count(&self) -> usize {
        self.tasks.values().filter(|t| t.is_running()).count()
    }

    /// Whether we can accept another background task.
    pub fn can_accept(&self) -> bool {
        self.running_count() < self.max_concurrent
    }

    /// Total number of tracked tasks.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether there are no tracked tasks.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Remove completed/failed/killed tasks older than the given age.
    pub fn cleanup_old(&mut self, max_age_secs: f64) {
        self.tasks
            .retain(|_, t| t.is_running() || t.runtime_seconds() < max_age_secs);
    }
}

impl Default for BackgroundAgentManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "background_agents_tests.rs"]
mod tests;
