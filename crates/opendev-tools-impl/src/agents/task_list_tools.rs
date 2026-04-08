//! Team shared task list tools: add, list, claim, and complete tasks.
//!
//! These tools let the team leader populate the shared task list and let
//! teammates claim and complete work items.

use std::collections::HashMap;
use std::sync::Arc;

use opendev_runtime::{TeamManager, TeamTask, TeamTaskList};
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

// ---------------------------------------------------------------------------
// TeamAddTaskTool
// ---------------------------------------------------------------------------

/// Tool for the team leader to add a new task to the shared task list.
#[derive(Debug)]
pub struct TeamAddTaskTool {
    team_manager: Arc<TeamManager>,
    task_list: Arc<TeamTaskList>,
}

impl TeamAddTaskTool {
    pub fn new(team_manager: Arc<TeamManager>, task_list: Arc<TeamTaskList>) -> Self {
        Self {
            team_manager,
            task_list,
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for TeamAddTaskTool {
    fn name(&self) -> &str {
        "TeamAddTask"
    }

    fn description(&self) -> &str {
        "Add a new task to the team's shared task list. Tasks start as 'pending' \
         and can be claimed by teammates. Optionally specify task dependencies \
         (IDs that must be completed first)."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Short task title (shown in list view)."
                },
                "description": {
                    "type": "string",
                    "description": "Full task description with acceptance criteria."
                },
                "dependencies": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of task IDs that must complete before this task is claimable."
                },
                "team_name": {
                    "type": "string",
                    "description": "Team name (optional when only one team is active)."
                }
            },
            "required": ["title", "description"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let title = match args.get("title").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::fail("Missing required parameter: title"),
        };
        let description = match args.get("description").and_then(|v| v.as_str()) {
            Some(d) => d,
            None => return ToolResult::fail("Missing required parameter: description"),
        };

        let team = match self.resolve_team(&args) {
            Some(t) => t,
            None => return ToolResult::fail("No active team found. Create a team first."),
        };

        let mut task = TeamTask::new(title, description);
        if let Some(deps) = args.get("dependencies").and_then(|v| v.as_array()) {
            task.dependencies = deps
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect();
        }

        match self.task_list.add_task(&team, task) {
            Ok(created) => ToolResult::ok(format!(
                "Task created.\n  ID: {}\n  Title: {}\n  Status: pending",
                created.id, created.title
            )),
            Err(e) => ToolResult::fail(format!("Failed to add task: {e}")),
        }
    }
}

impl TeamAddTaskTool {
    fn resolve_team(&self, args: &HashMap<String, serde_json::Value>) -> Option<String> {
        let teams = self.team_manager.list_teams();
        let filter = args.get("team_name").and_then(|v| v.as_str());
        if let Some(name) = filter {
            teams
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.name.clone())
        } else {
            teams.first().map(|t| t.name.clone())
        }
    }
}

// ---------------------------------------------------------------------------
// TeamListTasksTool
// ---------------------------------------------------------------------------

/// Tool to list all tasks in the team's shared task list.
#[derive(Debug)]
pub struct TeamListTasksTool {
    team_manager: Arc<TeamManager>,
    task_list: Arc<TeamTaskList>,
}

impl TeamListTasksTool {
    pub fn new(team_manager: Arc<TeamManager>, task_list: Arc<TeamTaskList>) -> Self {
        Self {
            team_manager,
            task_list,
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for TeamListTasksTool {
    fn name(&self) -> &str {
        "TeamListTasks"
    }

    fn description(&self) -> &str {
        "List all tasks in the team's shared task list. Shows status, assignee, \
         and dependencies. Use this to find pending tasks you can claim."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "status_filter": {
                    "type": "string",
                    "enum": ["all", "pending", "in_progress", "completed", "failed"],
                    "description": "Filter by status (default: all)."
                },
                "team_name": {
                    "type": "string",
                    "description": "Team name (optional when only one team is active)."
                }
            }
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let teams = self.team_manager.list_teams();
        let filter_name = args.get("team_name").and_then(|v| v.as_str());
        let team = if let Some(name) = filter_name {
            teams.iter().find(|t| t.name == name)
        } else {
            teams.first()
        };

        let team_name = match team {
            Some(t) => t.name.clone(),
            None => {
                return ToolResult::ok("No active team. No task list available.".to_string());
            }
        };

        let status_filter = args
            .get("status_filter")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let tasks = match self.task_list.list_tasks(&team_name) {
            Ok(t) => t,
            Err(e) => return ToolResult::fail(format!("Failed to list tasks: {e}")),
        };

        if tasks.is_empty() {
            return ToolResult::ok(format!("Team '{team_name}' has no tasks yet."));
        }

        let filtered: Vec<_> = tasks
            .iter()
            .filter(|t| status_filter == "all" || t.status.to_string() == status_filter)
            .collect();

        if filtered.is_empty() {
            return ToolResult::ok(format!("No tasks with status '{status_filter}'."));
        }

        let mut output = format!("Team '{}' tasks ({}):\n\n", team_name, filtered.len());
        for task in &filtered {
            let assignee = task.assigned_to.as_deref().unwrap_or("unassigned");
            let deps = if task.dependencies.is_empty() {
                String::new()
            } else {
                format!(" [deps: {}]", task.dependencies.join(", "))
            };
            output.push_str(&format!(
                "  [{status}] {id}  {title}\n    Assignee: {assignee}{deps}\n\n",
                status = task.status,
                id = task.id,
                title = task.title,
                assignee = assignee,
                deps = deps,
            ));
        }
        ToolResult::ok(output)
    }
}

// ---------------------------------------------------------------------------
// TeamClaimTaskTool
// ---------------------------------------------------------------------------

/// Tool for a teammate to claim a pending task.
#[derive(Debug)]
pub struct TeamClaimTaskTool {
    team_manager: Arc<TeamManager>,
    task_list: Arc<TeamTaskList>,
    /// Name of the agent using this tool (set at registration time).
    agent_name: String,
}

impl TeamClaimTaskTool {
    pub fn new(
        team_manager: Arc<TeamManager>,
        task_list: Arc<TeamTaskList>,
        agent_name: impl Into<String>,
    ) -> Self {
        Self {
            team_manager,
            task_list,
            agent_name: agent_name.into(),
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for TeamClaimTaskTool {
    fn name(&self) -> &str {
        "TeamClaimTask"
    }

    fn description(&self) -> &str {
        "Claim a pending task from the team task list, assigning it to yourself. \
         A task can only be claimed if it is 'pending' and all its dependencies \
         are 'completed'. Returns the task details on success."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "ID of the task to claim (from TeamListTasks)."
                },
                "team_name": {
                    "type": "string",
                    "description": "Team name (optional when only one team is active)."
                },
                "claimed_by": {
                    "type": "string",
                    "description": "Your teammate name. Required when calling as a teammate so the task is assigned to you."
                }
            },
            "required": ["task_id"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::fail("Missing required parameter: task_id"),
        };

        // Use explicit claimed_by from args if provided, fallback to self.agent_name
        let claimer = args
            .get("claimed_by")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.agent_name);

        let teams = self.team_manager.list_teams();
        let filter = args.get("team_name").and_then(|v| v.as_str());
        let team_name = if let Some(name) = filter {
            teams
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.name.clone())
        } else {
            teams.first().map(|t| t.name.clone())
        };

        let team_name = match team_name {
            Some(n) => n,
            None => return ToolResult::fail("No active team found."),
        };

        match self.task_list.claim_task(&team_name, task_id, claimer) {
            Ok(Some(task)) => ToolResult::ok(format!(
                "Task claimed successfully.\n  ID: {}\n  Title: {}\n  Status: in_progress\n  Assigned to: {}",
                task.id, task.title, claimer
            )),
            Ok(None) => ToolResult::fail(format!(
                "Task '{task_id}' could not be claimed. It may already be in progress, \
                 completed, or have unfinished dependencies."
            )),
            Err(e) => ToolResult::fail(format!("Failed to claim task: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// TeamCompleteTaskTool
// ---------------------------------------------------------------------------

/// Tool for a teammate to mark a task as completed or failed.
#[derive(Debug)]
pub struct TeamCompleteTaskTool {
    team_manager: Arc<TeamManager>,
    task_list: Arc<TeamTaskList>,
}

impl TeamCompleteTaskTool {
    pub fn new(team_manager: Arc<TeamManager>, task_list: Arc<TeamTaskList>) -> Self {
        Self {
            team_manager,
            task_list,
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for TeamCompleteTaskTool {
    fn name(&self) -> &str {
        "TeamCompleteTask"
    }

    fn description(&self) -> &str {
        "Mark a task as completed (success=true) or failed (success=false). \
         Completing a task may unblock dependent tasks for other teammates to claim."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "ID of the task to complete."
                },
                "success": {
                    "type": "boolean",
                    "description": "true = completed successfully, false = failed.",
                    "default": true
                },
                "team_name": {
                    "type": "string",
                    "description": "Team name (optional when only one team is active)."
                }
            },
            "required": ["task_id"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::fail("Missing required parameter: task_id"),
        };
        let success = args
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let teams = self.team_manager.list_teams();
        let filter = args.get("team_name").and_then(|v| v.as_str());
        let team_name = if let Some(name) = filter {
            teams
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.name.clone())
        } else {
            teams.first().map(|t| t.name.clone())
        };

        let team_name = match team_name {
            Some(n) => n,
            None => return ToolResult::fail("No active team found."),
        };

        match self.task_list.complete_task(&team_name, task_id, success) {
            Ok(Some(task)) => {
                let status = if success { "completed" } else { "failed" };
                ToolResult::ok(format!(
                    "Task '{task_id}' marked as {status}.\n  Title: {}",
                    task.title
                ))
            }
            Ok(None) => ToolResult::fail(format!("Task '{task_id}' not found.")),
            Err(e) => ToolResult::fail(format!("Failed to complete task: {e}")),
        }
    }
}
