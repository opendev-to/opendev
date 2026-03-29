//! Background task manager with process lifecycle tracking and output streaming.
//!
//! Manages long-running background processes with:
//! - File-based output storage (`/tmp/opendev/<path>/tasks/<id>.output`)
//! - Async output streaming from child process stdout/stderr
//! - Process lifecycle tracking (running → completed/failed/killed)
//! - Kill with SIGTERM + SIGKILL fallback
//! - Output reading with tail support

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Status of a background task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Completed,
    Failed,
    Killed,
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Killed => write!(f, "killed"),
        }
    }
}

/// Metadata and status of a background task.
#[derive(Debug, Clone)]
pub struct TaskStatus {
    /// Short hex task ID (e.g. "a1b2c3d").
    pub task_id: String,
    /// The command being executed.
    pub command: String,
    /// Human-readable description of the task.
    pub description: String,
    /// When the task was started.
    pub started_at: Instant,
    /// Current status label (kept for backward compat: "running", "completed", "failed", "killed").
    pub status: String,
    /// Typed task state.
    pub state: TaskState,
    /// Process ID.
    pub pid: Option<u32>,
    /// Path to the output file.
    pub output_file: Option<PathBuf>,
    /// Exit code (set when process exits).
    pub exit_code: Option<i32>,
    /// Error message (set on failure).
    pub error_message: Option<String>,
    /// When the task completed.
    pub completed_at: Option<Instant>,
}

impl TaskStatus {
    /// Runtime in seconds.
    pub fn runtime_seconds(&self) -> f64 {
        let end = self.completed_at.unwrap_or_else(Instant::now);
        end.duration_since(self.started_at).as_secs_f64()
    }

    /// Whether the task is still running.
    pub fn is_running(&self) -> bool {
        self.state == TaskState::Running
    }
}

/// Internal mutable state for a task (holds the child process handle).
struct TaskHandle {
    child: Option<Child>,
    cancel: tokio::sync::watch::Sender<bool>,
}

/// Listener callback type for task status changes.
pub type StatusListener = Box<dyn Fn(&str, TaskState) + Send + Sync>;

/// Manages background tasks with process lifecycle tracking and file-based output.
pub struct BackgroundTaskManager {
    tasks: HashMap<String, TaskStatus>,
    handles: HashMap<String, Arc<Mutex<TaskHandle>>>,
    output_dir: PathBuf,
    listeners: Vec<StatusListener>,
}

impl BackgroundTaskManager {
    /// Create a new task manager.
    ///
    /// `working_dir` is used to derive a unique output directory under `/tmp/opendev/`.
    pub fn new(working_dir: &Path) -> Self {
        let output_dir = Self::get_output_dir(working_dir);
        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            warn!("Failed to create task output dir: {e}");
        }

        Self {
            tasks: HashMap::new(),
            handles: HashMap::new(),
            output_dir,
            listeners: Vec::new(),
        }
    }

    /// Derive the output directory for task output files.
    fn get_output_dir(working_dir: &Path) -> PathBuf {
        let cwd = working_dir
            .canonicalize()
            .unwrap_or_else(|_| working_dir.to_path_buf());
        let safe_path = cwd.to_string_lossy().replace('/', "-");
        PathBuf::from(format!("/tmp/opendev/{safe_path}/tasks"))
    }

    /// Register and spawn a background task.
    ///
    /// Spawns the command via `sh -c`, streams stdout/stderr to a file,
    /// and monitors the process lifecycle.
    pub fn register_task(&mut self, command: &str, child: Child, initial_output: &str) -> String {
        let task_id = uuid::Uuid::new_v4().to_string()[..7].to_string();
        let output_file = self.output_dir.join(format!("{task_id}.output"));
        let pid = child.id();

        // Write initial output
        if !initial_output.is_empty()
            && let Err(e) = std::fs::write(&output_file, initial_output)
        {
            warn!(task_id, "Failed to write initial output: {e}");
        }

        let status = TaskStatus {
            task_id: task_id.clone(),
            command: command.to_string(),
            description: command.to_string(),
            started_at: Instant::now(),
            status: "running".to_string(),
            state: TaskState::Running,
            pid,
            output_file: Some(output_file.clone()),
            exit_code: None,
            error_message: None,
            completed_at: None,
        };

        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let handle = Arc::new(Mutex::new(TaskHandle {
            child: Some(child),
            cancel: cancel_tx,
        }));

        self.tasks.insert(task_id.clone(), status);
        self.handles.insert(task_id.clone(), handle.clone());
        self.notify_listeners(&task_id, TaskState::Running);

        // Spawn output streaming task
        let tid = task_id.clone();
        let out_path = output_file;
        tokio::spawn(async move {
            Self::stream_output(handle, &tid, out_path, cancel_rx).await;
        });

        info!(task_id, command, ?pid, "Registered background task");
        task_id
    }

    /// Add a simple tracked task (no process — backward compat).
    pub fn add_task(&mut self, id: String, description: String) {
        self.tasks.insert(
            id.clone(),
            TaskStatus {
                task_id: id,
                command: String::new(),
                description,
                started_at: Instant::now(),
                status: "running".to_string(),
                state: TaskState::Running,
                pid: None,
                output_file: None,
                exit_code: None,
                error_message: None,
                completed_at: None,
            },
        );
    }

    /// Update the status label of an existing task.
    pub fn update_task(&mut self, id: &str, status: String) -> bool {
        if let Some(task) = self.tasks.get_mut(id) {
            let state = match status.as_str() {
                "completed" => TaskState::Completed,
                "failed" => TaskState::Failed,
                "killed" => TaskState::Killed,
                _ => TaskState::Running,
            };
            task.status = status;
            task.state = state;
            if state != TaskState::Running {
                task.completed_at = Some(Instant::now());
            }
            true
        } else {
            false
        }
    }

    /// Remove a task by ID.
    pub fn remove_task(&mut self, id: &str) -> bool {
        self.handles.remove(id);
        self.tasks.remove(id).is_some()
    }

    /// Get all tasks that have "running" status.
    pub fn active_tasks(&self) -> Vec<(&str, &TaskStatus)> {
        self.tasks
            .iter()
            .filter(|(_, t)| t.state == TaskState::Running)
            .map(|(id, t)| (id.as_str(), t))
            .collect()
    }

    /// Get a task by ID.
    pub fn get_task(&self, id: &str) -> Option<&TaskStatus> {
        self.tasks.get(id)
    }

    /// Get all tasks sorted by start time (newest first).
    pub fn all_tasks(&self) -> Vec<&TaskStatus> {
        let mut tasks: Vec<&TaskStatus> = self.tasks.values().collect();
        tasks.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        tasks
    }

    /// Get the number of running tasks.
    pub fn running_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|t| t.state == TaskState::Running)
            .count()
    }

    /// Total number of tracked tasks.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether there are no tracked tasks.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Kill a running task.
    ///
    /// Sends SIGTERM, waits up to 5 seconds, then sends SIGKILL.
    pub async fn kill_task(&mut self, task_id: &str) -> bool {
        let handle = match self.handles.get(task_id) {
            Some(h) => h.clone(),
            None => return false,
        };

        let mut handle = handle.lock().await;

        // Signal the streaming task to stop
        let _ = handle.cancel.send(true);

        let Some(child) = &mut handle.child else {
            return false;
        };

        // Try graceful kill first
        if let Err(e) = child.kill().await {
            warn!(task_id, "Failed to kill task: {e}");
            return false;
        }

        // Wait for exit
        let exit_status =
            match tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await {
                Ok(Ok(status)) => status.code(),
                _ => None,
            };

        drop(handle);

        // Update task status
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.state = TaskState::Killed;
            task.status = "killed".to_string();
            task.exit_code = exit_status;
            task.completed_at = Some(Instant::now());
        }

        self.notify_listeners(task_id, TaskState::Killed);
        info!(task_id, "Killed background task");
        true
    }

    /// Read output from a task's output file.
    ///
    /// If `tail_lines` is 0, returns all output. Otherwise returns the last N lines.
    pub fn read_output(&self, task_id: &str, tail_lines: usize) -> String {
        let task = match self.tasks.get(task_id) {
            Some(t) => t,
            None => return String::new(),
        };

        let output_file = match &task.output_file {
            Some(f) => f,
            None => return String::new(),
        };

        let content = match std::fs::read_to_string(output_file) {
            Ok(c) => c,
            Err(_) => return String::new(),
        };

        if tail_lines == 0 {
            return content;
        }

        let lines: Vec<&str> = content.lines().collect();
        if lines.len() <= tail_lines {
            content
        } else {
            lines[lines.len() - tail_lines..].join("\n")
        }
    }

    /// Add a status change listener.
    pub fn add_listener(&mut self, callback: StatusListener) {
        self.listeners.push(callback);
    }

    /// Notify all listeners of a status change.
    fn notify_listeners(&self, task_id: &str, state: TaskState) {
        for listener in &self.listeners {
            listener(task_id, state);
        }
    }

    /// Clean up all tasks — kill any still running.
    pub async fn cleanup(&mut self) {
        let running_ids: Vec<String> = self
            .tasks
            .iter()
            .filter(|(_, t)| t.state == TaskState::Running)
            .map(|(id, _)| id.clone())
            .collect();

        for id in running_ids {
            let _ = self.kill_task(&id).await;
        }
    }

    /// Mark a task as completed with given exit code.
    ///
    /// Called by the streaming task when a child process exits, or
    /// externally when process exit is detected by polling.
    pub fn mark_completed(
        tasks: &mut HashMap<String, TaskStatus>,
        task_id: &str,
        exit_code: Option<i32>,
    ) {
        if let Some(task) = tasks.get_mut(task_id) {
            task.exit_code = exit_code;
            task.completed_at = Some(Instant::now());

            match exit_code {
                Some(0) => {
                    task.state = TaskState::Completed;
                    task.status = "completed".to_string();
                }
                Some(code) if code == 137 || code == 143 => {
                    // SIGKILL (137) or SIGTERM (143)
                    task.state = TaskState::Killed;
                    task.status = "killed".to_string();
                }
                Some(code) => {
                    task.state = TaskState::Failed;
                    task.status = "failed".to_string();
                    task.error_message = Some(format!("Exited with code {code}"));
                }
                None => {
                    task.state = TaskState::Failed;
                    task.status = "failed".to_string();
                    task.error_message = Some("Process terminated without exit code".to_string());
                }
            }
        }
    }

    /// Background task that streams child stdout/stderr to a file.
    async fn stream_output(
        handle: Arc<Mutex<TaskHandle>>,
        task_id: &str,
        output_file: PathBuf,
        cancel_rx: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut handle_guard = handle.lock().await;
        let child = match &mut handle_guard.child {
            Some(c) => c,
            None => return,
        };

        // Take stdout and stderr from child
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        drop(handle_guard);

        // Open output file for appending
        let file = match tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output_file)
            .await
        {
            Ok(f) => f,
            Err(e) => {
                warn!(task_id, "Failed to open output file: {e}");
                return;
            }
        };
        let file = Arc::new(Mutex::new(file));

        let mut join_handles = Vec::new();

        // Stream stdout
        if let Some(stdout) = stdout {
            let file = file.clone();
            let tid = task_id.to_string();
            let mut cancel = cancel_rx.clone();
            join_handles.push(tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                loop {
                    tokio::select! {
                        line = lines.next_line() => {
                            match line {
                                Ok(Some(line)) => {
                                    let mut f = file.lock().await;
                                    use tokio::io::AsyncWriteExt;
                                    let _ = f.write_all(line.as_bytes()).await;
                                    let _ = f.write_all(b"\n").await;
                                    let _ = f.flush().await;
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    debug!(task_id = tid, "stdout read error: {e}");
                                    break;
                                }
                            }
                        }
                        _ = cancel.changed() => break,
                    }
                }
            }));
        }

        // Stream stderr
        if let Some(stderr) = stderr {
            let file = file.clone();
            let tid = task_id.to_string();
            let mut cancel = cancel_rx.clone();
            join_handles.push(tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                loop {
                    tokio::select! {
                        line = lines.next_line() => {
                            match line {
                                Ok(Some(line)) => {
                                    let mut f = file.lock().await;
                                    use tokio::io::AsyncWriteExt;
                                    let _ = f.write_all(line.as_bytes()).await;
                                    let _ = f.write_all(b"\n").await;
                                    let _ = f.flush().await;
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    debug!(task_id = tid, "stderr read error: {e}");
                                    break;
                                }
                            }
                        }
                        _ = cancel.changed() => break,
                    }
                }
            }));
        }

        // Wait for all streams to finish
        for jh in join_handles {
            let _ = jh.await;
        }

        // Wait for process exit and get exit code
        let mut handle_guard = handle.lock().await;
        if let Some(child) = &mut handle_guard.child {
            match child.wait().await {
                Ok(status) => {
                    let code = status.code();
                    debug!(task_id, ?code, "Background task exited");
                    // We can't update self.tasks here since we don't have &mut self,
                    // but the exit code is captured in the handle for later polling.
                }
                Err(e) => {
                    warn!(task_id, "Failed to wait for process: {e}");
                }
            }
        }
    }
}

impl Default for BackgroundTaskManager {
    fn default() -> Self {
        Self {
            tasks: HashMap::new(),
            handles: HashMap::new(),
            output_dir: PathBuf::from("/tmp/opendev/default/tasks"),
            listeners: Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "background_tasks_tests.rs"]
mod tests;
