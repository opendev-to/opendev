//! Shared task list for agent teams.
//!
//! Manages work items that teammates can claim and complete. Tasks flow
//! through Pending → InProgress → Completed/Failed. Blocking on unmet
//! dependencies is supported: a task whose deps are not yet Completed
//! will not be claimable.
//!
//! Storage: `~/.opendev/tasks/{team-name}/tasks.json`

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::now_ms;

/// Status of a team task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamTaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

impl std::fmt::Display for TeamTaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// A single work item in the team's shared task list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTask {
    /// Short unique ID (12 hex chars).
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Full description / acceptance criteria.
    pub description: String,
    /// Current status.
    pub status: TeamTaskStatus,
    /// Which teammate claimed this task.
    pub assigned_to: Option<String>,
    /// IDs of tasks that must be Completed before this one is claimable.
    pub dependencies: Vec<String>,
    pub created_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub completed_at_ms: Option<u64>,
}

impl TeamTask {
    /// Create a new pending task with a fresh ID.
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string()[..12].to_string(),
            title: title.into(),
            description: description.into(),
            status: TeamTaskStatus::Pending,
            assigned_to: None,
            dependencies: Vec::new(),
            created_at_ms: now_ms(),
            started_at_ms: None,
            completed_at_ms: None,
        }
    }

    /// Whether this task is claimable given the current task list.
    ///
    /// A task is claimable if it is Pending and all its dependencies are Completed.
    pub fn is_claimable(&self, all_tasks: &[TeamTask]) -> bool {
        if self.status != TeamTaskStatus::Pending {
            return false;
        }
        self.dependencies.iter().all(|dep_id| {
            all_tasks
                .iter()
                .any(|t| &t.id == dep_id && t.status == TeamTaskStatus::Completed)
        })
    }
}

/// Manages shared task lists for all active teams.
///
/// In-memory cache is kept in sync with the on-disk JSON file.
/// Thread-safe via `RwLock`.
pub struct TeamTaskList {
    tasks_dir: PathBuf,
    cache: RwLock<HashMap<String, Vec<TeamTask>>>,
}

impl TeamTaskList {
    /// Create a new task list manager.
    ///
    /// `tasks_dir` is typically `~/.opendev/tasks/`.
    pub fn new(tasks_dir: PathBuf) -> Self {
        Self {
            tasks_dir,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Add a new task to a team's list. Returns the created task.
    pub fn add_task(&self, team_name: &str, task: TeamTask) -> std::io::Result<TeamTask> {
        let mut cache = self.cache.write().expect("TeamTaskList lock poisoned");
        self.ensure_loaded(team_name, &mut cache)?;

        let tasks = cache.entry(team_name.to_string()).or_default();
        let created = task.clone();
        tasks.push(task);
        self.persist(team_name, tasks)?;
        Ok(created)
    }

    /// List all tasks for a team, loading from disk if necessary.
    pub fn list_tasks(&self, team_name: &str) -> std::io::Result<Vec<TeamTask>> {
        let mut cache = self.cache.write().expect("TeamTaskList lock poisoned");
        self.ensure_loaded(team_name, &mut cache)?;
        Ok(cache.get(team_name).cloned().unwrap_or_default())
    }

    /// Claim a pending task, assigning it to `assignee`.
    ///
    /// Returns `Ok(Some(task))` on success, `Ok(None)` if the task is not
    /// claimable (wrong state or unmet dependencies).
    pub fn claim_task(
        &self,
        team_name: &str,
        task_id: &str,
        assignee: &str,
    ) -> std::io::Result<Option<TeamTask>> {
        let mut cache = self.cache.write().expect("TeamTaskList lock poisoned");
        self.ensure_loaded(team_name, &mut cache)?;

        let tasks = cache.entry(team_name.to_string()).or_default();
        // Check claimability against a snapshot before mutating
        let snapshot = tasks.clone();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            if !task.is_claimable(&snapshot) {
                return Ok(None);
            }
            task.status = TeamTaskStatus::InProgress;
            task.assigned_to = Some(assignee.to_string());
            task.started_at_ms = Some(now_ms());
            let result = task.clone();
            self.persist(team_name, tasks)?;
            return Ok(Some(result));
        }
        Ok(None)
    }

    /// Mark a task as completed or failed.
    ///
    /// Returns `Ok(Some(task))` on success, `Ok(None)` if the task was not found.
    pub fn complete_task(
        &self,
        team_name: &str,
        task_id: &str,
        success: bool,
    ) -> std::io::Result<Option<TeamTask>> {
        let mut cache = self.cache.write().expect("TeamTaskList lock poisoned");
        self.ensure_loaded(team_name, &mut cache)?;

        let tasks = cache.entry(team_name.to_string()).or_default();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = if success {
                TeamTaskStatus::Completed
            } else {
                TeamTaskStatus::Failed
            };
            task.completed_at_ms = Some(now_ms());
            let result = task.clone();
            self.persist(team_name, tasks)?;
            return Ok(Some(result));
        }
        Ok(None)
    }

    /// Delete all tasks for a team (called on team deletion).
    pub fn delete_team_tasks(&self, team_name: &str) -> std::io::Result<()> {
        let path = self.tasks_path(team_name);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        let mut cache = self.cache.write().expect("TeamTaskList lock poisoned");
        cache.remove(team_name);
        Ok(())
    }

    // -- Private helpers --

    fn tasks_path(&self, team_name: &str) -> PathBuf {
        self.tasks_dir.join(team_name).join("tasks.json")
    }

    fn ensure_loaded(
        &self,
        team_name: &str,
        cache: &mut HashMap<String, Vec<TeamTask>>,
    ) -> std::io::Result<()> {
        if cache.contains_key(team_name) {
            return Ok(());
        }
        let tasks = self.load_from_disk(team_name)?;
        cache.insert(team_name.to_string(), tasks);
        Ok(())
    }

    fn load_from_disk(&self, team_name: &str) -> std::io::Result<Vec<TeamTask>> {
        let path = self.tasks_path(team_name);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content).map_err(std::io::Error::other)
    }

    fn persist(&self, team_name: &str, tasks: &[TeamTask]) -> std::io::Result<()> {
        let path = self.tasks_path(team_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(tasks).map_err(std::io::Error::other)?;
        fs::write(path, json)?;
        Ok(())
    }
}

impl std::fmt::Debug for TeamTaskList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeamTaskList")
            .field("tasks_dir", &self.tasks_dir)
            .finish()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
