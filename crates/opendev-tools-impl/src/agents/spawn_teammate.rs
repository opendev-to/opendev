//! SpawnTeammateTool — spawns a named team member as a background agent
//! with full mailbox integration.
//!
//! Unlike `SpawnSubagentTool` (which spawns ephemeral subagents), teammates:
//!   1. Are registered in the team config with a fixed name.
//!   2. Receive their inbox (`Mailbox`) so incoming messages are injected into
//!      the LLM message history on each iteration.
//!   3. Carry extra system-prompt context about the team and their role.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use opendev_runtime::{Mailbox, TeamManager, TeamMemberStatus};
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::WorktreeManager;

use super::events::{BackgroundProgressCallback, SubagentEvent};

/// Spawns a registered team member as a background agent with mailbox support.
#[derive(Debug)]
pub struct SpawnTeammateTool {
    team_manager: Arc<TeamManager>,
    manager: Arc<opendev_agents::SubagentManager>,
    tool_registry: Arc<opendev_tools_core::ToolRegistry>,
    http_client: Arc<opendev_http::AdaptedClient>,
    session_dir: PathBuf,
    parent_model: String,
    working_dir: String,
    parent_max_tokens: u64,
    parent_reasoning_effort: Option<String>,
    event_tx: Option<mpsc::UnboundedSender<SubagentEvent>>,
    debug_logger: Option<Arc<opendev_runtime::SessionDebugLogger>>,
    worktree_manager: Option<Arc<Mutex<WorktreeManager>>>,
}

impl SpawnTeammateTool {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        team_manager: Arc<TeamManager>,
        manager: Arc<opendev_agents::SubagentManager>,
        tool_registry: Arc<opendev_tools_core::ToolRegistry>,
        http_client: Arc<opendev_http::AdaptedClient>,
        session_dir: PathBuf,
        parent_model: impl Into<String>,
        working_dir: impl Into<String>,
    ) -> Self {
        Self {
            team_manager,
            manager,
            tool_registry,
            http_client,
            session_dir,
            parent_model: parent_model.into(),
            working_dir: working_dir.into(),
            parent_max_tokens: 16384,
            parent_reasoning_effort: None,
            event_tx: None,
            debug_logger: None,
            worktree_manager: None,
        }
    }

    pub fn with_event_sender(mut self, tx: mpsc::UnboundedSender<SubagentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    pub fn with_parent_max_tokens(mut self, max_tokens: u64) -> Self {
        self.parent_max_tokens = max_tokens;
        self
    }

    pub fn with_parent_reasoning_effort(mut self, effort: Option<String>) -> Self {
        self.parent_reasoning_effort = effort;
        self
    }

    pub fn with_debug_logger(mut self, logger: Arc<opendev_runtime::SessionDebugLogger>) -> Self {
        self.debug_logger = Some(logger);
        self
    }

    pub fn with_worktree_manager(mut self, manager: Arc<Mutex<WorktreeManager>>) -> Self {
        self.worktree_manager = Some(manager);
        self
    }
}

#[async_trait::async_trait]
impl BaseTool for SpawnTeammateTool {
    fn name(&self) -> &str {
        "SpawnTeammate"
    }

    fn description(&self) -> &str {
        "Spawn a teammate as a background agent. The teammate runs independently \
         with its own context window and receives messages via its mailbox.\n\n\
         You can call this directly — the team is auto-created if it doesn't exist, \
         and the member is auto-registered if not already present. Just provide \
         team_name, member_name, agent_type, and task.\n\n\
         CRITICAL: To spawn multiple teammates in PARALLEL, make all SpawnTeammate \
         calls in the SAME response. Sequential responses = sequential execution.\n\n\
         The teammate will:\n\
         - Check its mailbox for messages from you and other teammates\n\
         - Send results back via SendMessage when done\n\
         - Run until its task is complete or it receives a shutdown request"
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "team_name": {
                    "type": "string",
                    "description": "Short name for the team (e.g., 'research', 'refactor'). Auto-created if it doesn't exist."
                },
                "member_name": {
                    "type": "string",
                    "description": "Unique name for this teammate (e.g., 'explorer', 'analyzer')."
                },
                "agent_type": {
                    "type": "string",
                    "description": "Subagent type (e.g., 'Explore', 'Planner'). Required for new members, ignored if member already registered."
                },
                "task": {
                    "type": "string",
                    "description": "Detailed task description for this teammate. Required for new members, ignored if member already registered."
                },
                "model": {
                    "type": "string",
                    "description": "Optional model override for this teammate."
                },
                "worktree": {
                    "type": "boolean",
                    "description": "If true, create an isolated git worktree for this teammate. Recommended when the teammate modifies files to prevent conflicts.",
                    "default": false
                }
            },
            "required": ["team_name", "member_name"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let team_name = match args.get("team_name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::fail("Missing required parameter: team_name"),
        };
        let member_name = match args.get("member_name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::fail("Missing required parameter: member_name"),
        };
        let model_override = args.get("model").and_then(|v| v.as_str());
        let inline_agent_type = args.get("agent_type").and_then(|v| v.as_str());
        let inline_task = args.get("task").and_then(|v| v.as_str());
        let use_worktree = args
            .get("worktree")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Auto-create team if it doesn't exist
        if self.team_manager.get_team(team_name).is_none() {
            let session_id = ctx.session_id.as_deref().unwrap_or("unknown");
            if let Err(e) = self
                .team_manager
                .create_team(team_name, "leader", session_id)
            {
                return ToolResult::fail(format!("Failed to auto-create team: {e}"));
            }
            info!(team = %team_name, "Auto-created team");
        }

        // Auto-register member if not already in the team
        let member = if let Some(existing) = self
            .team_manager
            .get_team(team_name)
            .and_then(|t| t.members.iter().find(|m| m.name == member_name).cloned())
        {
            existing
        } else {
            // Need agent_type and task for new members
            let agent_type = match inline_agent_type {
                Some(t) => t,
                None => {
                    return ToolResult::fail(
                        "New member requires 'agent_type' parameter (e.g., 'Explore', 'Planner').",
                    );
                }
            };
            let task = match inline_task {
                Some(t) => t,
                None => {
                    return ToolResult::fail(
                        "New member requires 'task' parameter with a detailed task description.",
                    );
                }
            };
            let task_id = uuid::Uuid::new_v4().to_string()[..12].to_string();
            let new_member = opendev_runtime::TeamMember {
                name: member_name.to_string(),
                agent_type: agent_type.to_string(),
                task_id,
                task: task.to_string(),
                status: TeamMemberStatus::Idle,
                joined_at_ms: opendev_runtime::now_ms(),
            };
            let _ = self.team_manager.add_member(team_name, new_member.clone());
            info!(
                team = %team_name,
                member = %member_name,
                agent_type = %agent_type,
                "Auto-registered member"
            );
            new_member
        };

        // Validate the agent type exists
        if self.manager.get(&member.agent_type).is_none() {
            return ToolResult::fail(format!(
                "Unknown agent type '{}' for member '{member_name}'",
                member.agent_type
            ));
        }

        // Build the teammate's task (inject team context into the task)
        let leader = self
            .team_manager
            .get_team(team_name)
            .map(|t| t.leader.clone())
            .unwrap_or_else(|| "leader".to_string());
        let team_context = build_team_context(team_name, member_name, &leader, &member.task);

        // Create the mailbox for this teammate
        let team_dir = self.team_manager.team_dir(team_name);
        let mailbox = Mailbox::new(&team_dir, member_name);

        // Create unique IDs
        let task_id = uuid::Uuid::new_v4().to_string()[..12].to_string();
        let session_id = uuid::Uuid::new_v4().to_string();
        let interrupt_token = opendev_runtime::InterruptToken::new();

        info!(
            team = %team_name,
            member = %member_name,
            agent_type = %member.agent_type,
            task_id = %task_id,
            "Spawning teammate"
        );

        // Update status to Busy
        self.team_manager
            .update_member_status(team_name, member_name, TeamMemberStatus::Busy);

        // Notify TUI
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(SubagentEvent::BackgroundSpawned {
                task_id: task_id.clone(),
                agent_type: member.agent_type.clone(),
                query: member.task.clone(),
                description: format!("{member_name} ({team_name})"),
                session_id: session_id.clone(),
                interrupt_token: interrupt_token.clone(),
            });
        }

        // Clone everything for the async task
        let manager = Arc::clone(&self.manager);
        let registry = Arc::clone(&self.tool_registry);
        let http = Arc::clone(&self.http_client);
        let event_tx = self.event_tx.clone();
        let parent_model = self.parent_model.clone();
        let parent_max_tokens = self.parent_max_tokens;
        let reasoning_effort = self.parent_reasoning_effort.clone();
        let debug_logger_arc = self.debug_logger.clone();
        let session_dir = self.session_dir.clone();
        let team_manager = Arc::clone(&self.team_manager);
        let base_working_dir = if let Some(ref wd) = ctx.working_dir.to_str() {
            if ctx.working_dir.as_os_str().is_empty()
                || ctx.working_dir == std::path::Path::new(".")
            {
                self.working_dir.clone()
            } else {
                wd.to_string()
            }
        } else {
            self.working_dir.clone()
        };

        // Create worktree if requested
        let working_dir = if use_worktree {
            if let Some(ref wt_mgr) = self.worktree_manager {
                let wt_name = format!("{team_name}-{member_name}");
                match wt_mgr
                    .lock()
                    .await
                    .create(Some(&wt_name), None, "HEAD")
                    .await
                {
                    Ok(info) => {
                        let wt_path = info.path.clone();
                        info!(
                            team = %team_name,
                            member = %member_name,
                            worktree = %wt_path,
                            "Created worktree for teammate"
                        );
                        wt_path
                    }
                    Err(e) => {
                        tracing::warn!(
                            team = %team_name,
                            member = %member_name,
                            error = %e,
                            "Failed to create worktree, falling back to shared working dir"
                        );
                        base_working_dir
                    }
                }
            } else {
                tracing::warn!(
                    "worktree requested but WorktreeManager not configured, using shared working dir"
                );
                base_working_dir
            }
        } else {
            base_working_dir
        };
        let team_name_owned = team_name.to_string();
        let member_name_owned = member_name.to_string();
        let agent_type_owned = member.agent_type.clone();
        let task_id_clone = task_id.clone();
        let cancel_token = CancellationToken::new();
        let model_override_owned = model_override.map(|s| s.to_string());

        tokio::spawn(async move {
            let progress: Arc<dyn opendev_agents::SubagentProgressCallback> =
                if let Some(ref tx) = event_tx {
                    Arc::new(BackgroundProgressCallback::new(
                        tx.clone(),
                        task_id_clone.clone(),
                    ))
                } else {
                    Arc::new(opendev_agents::NoopProgressCallback)
                };

            let result = manager
                .spawn(
                    &agent_type_owned,
                    &team_context,
                    &parent_model,
                    registry,
                    http,
                    &working_dir,
                    progress,
                    None,
                    None,
                    parent_max_tokens,
                    reasoning_effort,
                    Some(cancel_token),
                    debug_logger_arc.as_deref(),
                    model_override_owned.as_deref(),
                    Some(&mailbox),
                )
                .await;

            // Update team member status on completion
            let (success, summary, full_result) = match result {
                Ok(ref run_result) => {
                    let ok =
                        run_result.agent_result.success && !run_result.agent_result.interrupted;
                    let summary = if run_result.agent_result.content.len() > 200 {
                        format!(
                            "{}...",
                            opendev_runtime::safe_truncate(&run_result.agent_result.content, 200)
                        )
                    } else {
                        run_result.agent_result.content.clone()
                    };
                    (ok, summary, run_result.agent_result.content.clone())
                }
                Err(ref e) => (false, e.to_string(), String::new()),
            };

            let new_status = if success {
                TeamMemberStatus::Done
            } else {
                TeamMemberStatus::Failed
            };
            team_manager.update_member_status(&team_name_owned, &member_name_owned, new_status);

            // Save child session
            if let Ok(ref run_result) = result {
                let child_mgr = opendev_history::SessionManager::new(session_dir);
                if let Ok(child_mgr) = child_mgr {
                    let mut session = opendev_models::session::Session::new();
                    session.id = task_id_clone.clone();
                    session.working_directory = Some(working_dir);
                    session.metadata.insert(
                        "title".to_string(),
                        serde_json::json!(format!("{member_name_owned} ({team_name_owned})")),
                    );
                    session
                        .metadata
                        .insert("team_name".to_string(), serde_json::json!(&team_name_owned));
                    session.metadata.insert(
                        "member_name".to_string(),
                        serde_json::json!(&member_name_owned),
                    );
                    let messages = opendev_history::message_convert::api_values_to_chatmessages(
                        &run_result.agent_result.messages,
                    );
                    session.messages = messages;
                    let _ = child_mgr.save_session(&session);
                }
            }

            if let Some(ref tx) = event_tx {
                let tool_call_count = result.as_ref().map(|r| r.tool_call_count).unwrap_or(0);
                let _ = tx.send(SubagentEvent::BackgroundCompleted {
                    task_id: task_id_clone,
                    success,
                    result_summary: summary,
                    full_result,
                    cost_usd: 0.0,
                    tool_call_count,
                });
            }
        });

        ToolResult::ok(format!(
            "Teammate '{member_name}' spawned.\n\
             task_id: {task_id}\n\
             Team: {team_name}\n\
             Agent type: {agent_type}\n\n\
             Running in background. They will check their mailbox for messages from you.\n\
             Use SendMessage to communicate with them.",
            member_name = member_name,
            task_id = task_id,
            team_name = team_name,
            agent_type = member.agent_type,
        ))
    }
}

/// Build the full task string for a teammate, injecting team context.
fn build_team_context(team_name: &str, member_name: &str, leader_name: &str, task: &str) -> String {
    format!(
        "You are **{member_name}**, a member of team **{team_name}**.\n\
         Team leader: {leader_name}\n\n\
         ## Your Task\n\
         {task}\n\n\
         ## Team Collaboration Guidelines\n\
         - Use `CheckMailbox(agent_name=\"{member_name}\")` to read messages from your team leader and teammates.\n\
           Check it periodically (every few steps) to stay in sync.\n\
         - Use `SendMessage` to send updates or ask for help. Always include `team_name: \"{team_name}\"`.\n\
         - Use `TeamListTasks` to see the shared task list.\n\
         - Use `TeamClaimTask(task_id=..., claimed_by=\"{member_name}\")` to claim a pending task.\n\
         - Use `TeamCompleteTask` to mark your task done when finished.\n\
         - If you receive a shutdown request via CheckMailbox, wrap up and stop.\n\n\
         ## Progress Tracking\n\
         - Use `TeamClaimTask` / `TeamCompleteTask` to track your progress on team tasks.\n\
         - Use `SendMessage` to report detailed status updates to the team leader.\n\
         - Do NOT call `TodoWrite` — only the team leader manages the master todo list.\n\
         - If the leader asks you to update a specific todo, use `TaskUpdate(id, status=\"in_progress\")` or `TaskUpdate(id, status=\"completed\")`.\n\n\
         ## Planning\n\
         - For complex tasks, if the leader requests a plan via SendMessage, use `EnterPlanMode` to present it for approval.\n\
         - For simple tasks, proceed directly with implementation.\n\n\
         Complete your task, then report your results via SendMessage to the leader.",
        member_name = member_name,
        team_name = team_name,
        leader_name = leader_name,
        task = task,
    )
}
