//! Agent task lifecycle manager.
//!
//! Tracks background agents, team members, and their state transitions.
//! UI-agnostic — can be used from TUI, REPL, or web backends.
//!
//! State machine:
//! ```text
//! Pending → Running → Completed
//!                   → Failed
//!        → Killed  (from any state)
//! ```

mod types;

pub use types::*;

use std::collections::HashMap;
use std::sync::RwLock;

use tokio::sync::mpsc;
use tracing::warn;

use crate::InterruptToken;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Manages the lifecycle of background agent tasks.
///
/// Thread-safe via `RwLock`. All state transition methods are idempotent —
/// calling `kill_task` on an already-killed task is a no-op.
pub struct TaskManager {
    tasks: RwLock<HashMap<String, TaskInfo>>,
    interrupt_tokens: RwLock<HashMap<String, InterruptToken>>,
    /// Maximum number of concurrently running tasks.
    pub max_concurrent: usize,
    event_tx: Option<mpsc::UnboundedSender<TaskManagerEvent>>,
}

impl std::fmt::Debug for TaskManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.tasks.read().map(|t| t.len()).unwrap_or(0);
        f.debug_struct("TaskManager")
            .field("task_count", &count)
            .field("max_concurrent", &self.max_concurrent)
            .finish()
    }
}

impl TaskManager {
    /// Create a new task manager with the given concurrency limit.
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            interrupt_tokens: RwLock::new(HashMap::new()),
            max_concurrent,
            event_tx: None,
        }
    }

    /// Attach an event channel for state transition notifications.
    pub fn with_event_sender(mut self, tx: mpsc::UnboundedSender<TaskManagerEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    // -- State transitions --

    /// Register a new task. Returns the task_id.
    pub fn create_task(&self, info: TaskInfo) -> String {
        let task_id = info.task_id.clone();
        self.emit(TaskManagerEvent::StateChanged {
            task_id: task_id.clone(),
            old: TaskState::Pending,
            new: info.state,
        });
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        tasks.insert(task_id.clone(), info);
        task_id
    }

    /// Transition Pending → Running.
    pub fn start_task(&self, task_id: &str) {
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            if task.state != TaskState::Pending {
                return;
            }
            let old = task.state;
            task.state = TaskState::Running;
            task.started_at_ms = Some(now_ms());
            drop(tasks);
            self.emit(TaskManagerEvent::StateChanged {
                task_id: task_id.to_string(),
                old,
                new: TaskState::Running,
            });
        }
    }

    /// Transition Running → Completed or Failed.
    pub fn complete_task(&self, task_id: &str, success: bool, summary: &str, full_result: &str) {
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            if task.state.is_terminal() {
                return; // idempotent
            }
            let old = task.state;
            let new_state = if success {
                TaskState::Completed
            } else {
                TaskState::Failed
            };
            task.state = new_state;
            task.completed_at_ms = Some(now_ms());
            task.result_summary = Some(summary.to_string());
            task.full_result = Some(full_result.to_string());
            if task.evict_after_ms.is_none() && !task.retain {
                task.evict_after_ms = Some(now_ms() + EVICT_GRACE_MS);
            }
            drop(tasks);
            self.emit(TaskManagerEvent::StateChanged {
                task_id: task_id.to_string(),
                old,
                new: new_state,
            });
        }
    }

    /// Transition Running → Failed with error message.
    pub fn fail_task(&self, task_id: &str, error: &str) {
        self.complete_task(task_id, false, error, "");
    }

    /// Transition any → Killed. Also cancels the interrupt token if set.
    pub fn kill_task(&self, task_id: &str) {
        // Cancel the interrupt token first (outside task lock)
        if let Ok(tokens) = self.interrupt_tokens.read()
            && let Some(token) = tokens.get(task_id)
        {
            token.request();
        }

        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            if task.state.is_terminal() {
                return; // idempotent
            }
            let old = task.state;
            task.state = TaskState::Killed;
            task.completed_at_ms = Some(now_ms());
            if task.evict_after_ms.is_none() && !task.retain {
                task.evict_after_ms = Some(now_ms() + EVICT_GRACE_MS);
            }
            drop(tasks);
            self.emit(TaskManagerEvent::StateChanged {
                task_id: task_id.to_string(),
                old,
                new: TaskState::Killed,
            });
        }
    }

    /// Mark a task as backgrounded.
    pub fn background_task(&self, task_id: &str) {
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            task.is_backgrounded = true;
        }
    }

    // -- Progress tracking --

    /// Update progress counters for a task.
    pub fn update_progress(
        &self,
        task_id: &str,
        tool_name: &str,
        activity: Option<ToolActivity>,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            task.tool_call_count += 1;
            task.current_tool = Some(tool_name.to_string());
            task.input_tokens += input_tokens;
            task.output_tokens += output_tokens;

            if let Some(act) = activity {
                task.last_activity = Some(act.clone());
                task.recent_activities.push(act);
                if task.recent_activities.len() > MAX_RECENT_ACTIVITIES {
                    task.recent_activities.remove(0);
                }
            }

            let tool_count = task.tool_call_count;
            drop(tasks);
            self.emit(TaskManagerEvent::Progress {
                task_id: task_id.to_string(),
                tool_name: tool_name.to_string(),
                tool_count,
            });
        }
    }

    /// Append a line to the rolling activity log.
    pub fn push_activity(&self, task_id: &str, line: String) {
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            task.activity_log.push(line);
            if task.activity_log.len() > MAX_ACTIVITY_LOG {
                task.activity_log.remove(0);
            }
        }
    }

    // -- Message queue --

    /// Queue a message for delivery to a task's agent.
    pub fn queue_message(&self, task_id: &str, msg: PendingMessage) {
        let from = msg.from_agent.clone();
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            task.pending_messages.push(msg);
            drop(tasks);
            self.emit(TaskManagerEvent::MessageReceived {
                task_id: task_id.to_string(),
                from,
            });
        }
    }

    /// Drain all pending messages for a task.
    pub fn drain_messages(&self, task_id: &str) -> Vec<PendingMessage> {
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            std::mem::take(&mut task.pending_messages)
        } else {
            Vec::new()
        }
    }

    // -- Queries --

    /// Get a snapshot of a task's info.
    pub fn get(&self, task_id: &str) -> Option<TaskInfo> {
        let tasks = self.tasks.read().expect("TaskManager lock poisoned");
        tasks.get(task_id).cloned()
    }

    /// List all tasks (snapshot).
    pub fn list(&self) -> Vec<TaskInfo> {
        let tasks = self.tasks.read().expect("TaskManager lock poisoned");
        tasks.values().cloned().collect()
    }

    /// Count of tasks in Running state.
    pub fn running_count(&self) -> usize {
        let tasks = self.tasks.read().expect("TaskManager lock poisoned");
        tasks
            .values()
            .filter(|t| t.state == TaskState::Running)
            .count()
    }

    /// Whether more tasks can be accepted (under the concurrency limit).
    pub fn can_accept(&self) -> bool {
        self.running_count() < self.max_concurrent
    }

    // -- Interrupt tokens --

    /// Associate an interrupt token with a task.
    pub fn set_interrupt_token(&self, task_id: &str, token: InterruptToken) {
        let mut tokens = self.interrupt_tokens.write().expect("tokens lock poisoned");
        tokens.insert(task_id.to_string(), token);
    }

    /// Get the interrupt token for a task.
    pub fn get_interrupt_token(&self, task_id: &str) -> Option<InterruptToken> {
        let tokens = self.interrupt_tokens.read().expect("tokens lock poisoned");
        tokens.get(task_id).cloned()
    }

    // -- Notification & eviction --

    /// Atomically check and set the notified flag.
    /// Returns `true` if the task was previously un-notified (first call wins).
    pub fn mark_notified(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            if task.notified {
                return false; // already notified
            }
            task.notified = true;
            true
        } else {
            false
        }
    }

    /// Try to evict a terminal task from state.
    /// Returns `true` if the task was evicted.
    ///
    /// Conditions: terminal state + notified + past evict_after + !retain.
    pub fn try_evict(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        let should_evict = tasks.get(task_id).is_some_and(|t| {
            t.state.is_terminal()
                && t.notified
                && !t.retain
                && t.evict_after_ms
                    .is_some_and(|deadline| now_ms() >= deadline)
        });
        if should_evict {
            tasks.remove(task_id);
            let mut tokens = self.interrupt_tokens.write().expect("tokens lock poisoned");
            tokens.remove(task_id);
            true
        } else {
            false
        }
    }

    /// Set the retain flag (blocks eviction while UI is viewing).
    pub fn set_retain(&self, task_id: &str, retain: bool) {
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            task.retain = retain;
            if !retain && task.state.is_terminal() && task.evict_after_ms.is_none() {
                task.evict_after_ms = Some(now_ms() + EVICT_GRACE_MS);
            }
        }
    }

    /// Schedule eviction at a specific timestamp.
    pub fn set_evict_after(&self, task_id: &str, ms: u64) {
        let mut tasks = self.tasks.write().expect("TaskManager lock poisoned");
        if let Some(task) = tasks.get_mut(task_id) {
            task.evict_after_ms = Some(ms);
        }
    }

    // -- Internal --

    fn emit(&self, event: TaskManagerEvent) {
        if let Some(ref tx) = self.event_tx
            && tx.send(event).is_err()
        {
            warn!("TaskManager event channel closed");
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
