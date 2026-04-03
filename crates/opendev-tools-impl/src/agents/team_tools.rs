//! Team management tools: create_team, send_message, delete_team.

use std::collections::HashMap;
use std::sync::Arc;

use opendev_runtime::{
    Mailbox, MailboxMessage, MessageType, TeamManager, TeamMember, TeamMemberStatus,
};
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};
use tokio::sync::mpsc;
use tracing::info;

use super::events::SubagentEvent;

// ---------------------------------------------------------------------------
// CreateTeamTool
// ---------------------------------------------------------------------------

/// Tool that creates a named team of background agents.
#[derive(Debug)]
pub struct CreateTeamTool {
    team_manager: Arc<TeamManager>,
    event_tx: Option<mpsc::UnboundedSender<SubagentEvent>>,
}

impl CreateTeamTool {
    pub fn new(team_manager: Arc<TeamManager>) -> Self {
        Self {
            team_manager,
            event_tx: None,
        }
    }

    pub fn with_event_sender(mut self, tx: mpsc::UnboundedSender<SubagentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }
}

#[async_trait::async_trait]
impl BaseTool for CreateTeamTool {
    fn name(&self) -> &str {
        "TeamCreate"
    }

    fn description(&self) -> &str {
        "Create a named team of agents that work together. Each member runs as a \
         background agent with its own task. Members communicate via send_message. \
         Use for complex tasks requiring multiple specialized agents in parallel."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "team_name": {
                    "type": "string",
                    "description": "Short name for the team (e.g., 'analysis', 'refactor-auth')."
                },
                "members": {
                    "type": "array",
                    "description": "List of team members to spawn.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "Member name (used for send_message routing)."
                            },
                            "agent_type": {
                                "type": "string",
                                "description": "Subagent type to spawn (e.g., 'Explore', 'Planner')."
                            },
                            "task": {
                                "type": "string",
                                "description": "Task description for this member."
                            }
                        },
                        "required": ["name", "agent_type", "task"]
                    }
                }
            },
            "required": ["team_name", "members"]
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

        let members = match args.get("members").and_then(|v| v.as_array()) {
            Some(m) => m,
            None => return ToolResult::fail("Missing required parameter: members"),
        };

        if members.is_empty() {
            return ToolResult::fail("Team must have at least one member");
        }

        let session_id = ctx.session_id.as_deref().unwrap_or("unknown");

        // Create the team
        match self
            .team_manager
            .create_team(team_name, "leader", session_id)
        {
            Ok(_) => {}
            Err(e) => return ToolResult::fail(format!("Failed to create team: {e}")),
        }

        let mut member_info = Vec::new();

        for member_val in members {
            let name = member_val
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unnamed");
            let agent_type = member_val
                .get("agent_type")
                .and_then(|v| v.as_str())
                .unwrap_or("General");
            let task = member_val
                .get("task")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let task_id = uuid::Uuid::new_v4().to_string()[..12].to_string();

            // Register member in team config
            let team_member = TeamMember {
                name: name.to_string(),
                agent_type: agent_type.to_string(),
                task_id: task_id.clone(),
                task: task.to_string(),
                status: TeamMemberStatus::Idle,
                joined_at_ms: opendev_runtime::now_ms(),
            };
            let _ = self.team_manager.add_member(team_name, team_member);

            member_info.push(format!("- {name} ({agent_type}): task_id={task_id}"));

            info!(
                team = %team_name,
                member = %name,
                agent_type = %agent_type,
                task_id = %task_id,
                "Team member registered"
            );
        }

        // Notify TUI
        if let Some(ref tx) = self.event_tx {
            let member_names: Vec<String> = members
                .iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .collect();
            let _ = tx.send(SubagentEvent::TeamCreated {
                team_id: team_name.to_string(),
                leader: "leader".to_string(),
                members: member_names,
            });
        }

        let member_list = member_info.join("\n");
        ToolResult::ok(format!(
            "Team '{team_name}' created with {} members.\n\n{member_list}\n\n\
             Members are registered. To start them, spawn each as a background agent \
             with run_in_background=true. Use send_message to communicate between members.",
            members.len()
        ))
    }
}

// ---------------------------------------------------------------------------
// SendMessageTool
// ---------------------------------------------------------------------------

/// Tool for sending messages between team agents.
#[derive(Debug)]
pub struct SendMessageTool {
    team_manager: Arc<TeamManager>,
    event_tx: Option<mpsc::UnboundedSender<SubagentEvent>>,
}

impl SendMessageTool {
    pub fn new(team_manager: Arc<TeamManager>) -> Self {
        Self {
            team_manager,
            event_tx: None,
        }
    }

    pub fn with_event_sender(mut self, tx: mpsc::UnboundedSender<SubagentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }
}

#[async_trait::async_trait]
impl BaseTool for SendMessageTool {
    fn name(&self) -> &str {
        "SendMessage"
    }

    fn description(&self) -> &str {
        "Send a message to a team member by name, or broadcast to all members with to=\"*\". \
         Use to coordinate work, share findings, or request actions from teammates."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Recipient name, or \"*\" to broadcast to all team members."
                },
                "message": {
                    "type": "string",
                    "description": "Message content."
                },
                "team_name": {
                    "type": "string",
                    "description": "Team name (required if in multiple teams)."
                }
            },
            "required": ["to", "message"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let to = match args.get("to").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::fail("Missing required parameter: to"),
        };

        let message = match args.get("message").and_then(|v| v.as_str()) {
            Some(m) => m,
            None => return ToolResult::fail("Missing required parameter: message"),
        };

        // Find the team
        let teams = self.team_manager.list_teams();
        let team_name_filter = args.get("team_name").and_then(|v| v.as_str());
        let team = if let Some(name) = team_name_filter {
            teams.iter().find(|t| t.name == name)
        } else {
            teams.first()
        };

        let team = match team {
            Some(t) => t,
            None => return ToolResult::fail("No active team found. Create a team first."),
        };

        let team_dir = self.team_manager.team_dir(&team.name);
        let sender = "leader"; // The main agent is always the leader

        let msg = MailboxMessage {
            id: uuid::Uuid::new_v4().to_string(),
            from: sender.to_string(),
            content: message.to_string(),
            timestamp_ms: opendev_runtime::now_ms(),
            read: false,
            msg_type: MessageType::Text,
        };

        if to == "*" {
            // Broadcast to all members
            let mut sent = 0;
            for member in &team.members {
                let mailbox = Mailbox::new(&team_dir, &member.name);
                if let Err(e) = mailbox.send(msg.clone()) {
                    tracing::warn!(member = %member.name, error = %e, "Failed to send to member");
                } else {
                    sent += 1;
                }
            }

            // Notify TUI
            if let Some(ref tx) = self.event_tx {
                let _ = tx.send(SubagentEvent::TeamMessageSent {
                    from: sender.to_string(),
                    to: "*".to_string(),
                    preview: message.chars().take(50).collect(),
                });
            }

            ToolResult::ok(format!(
                "Broadcast sent to {sent}/{} members.",
                team.members.len()
            ))
        } else {
            // Send to specific member
            let member = team.members.iter().find(|m| m.name == to);
            if member.is_none() {
                return ToolResult::fail(format!(
                    "Member '{to}' not found in team '{}'. Available: {}",
                    team.name,
                    team.members
                        .iter()
                        .map(|m| m.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            let mailbox = Mailbox::new(&team_dir, to);
            if let Err(e) = mailbox.send(msg) {
                return ToolResult::fail(format!("Failed to send message: {e}"));
            }

            // Notify TUI
            if let Some(ref tx) = self.event_tx {
                let _ = tx.send(SubagentEvent::TeamMessageSent {
                    from: sender.to_string(),
                    to: to.to_string(),
                    preview: message.chars().take(50).collect(),
                });
            }

            ToolResult::ok(format!("Message sent to '{to}'."))
        }
    }
}

// ---------------------------------------------------------------------------
// DeleteTeamTool
// ---------------------------------------------------------------------------

/// Tool for deleting a team and cleaning up resources.
#[derive(Debug)]
pub struct DeleteTeamTool {
    team_manager: Arc<TeamManager>,
    event_tx: Option<mpsc::UnboundedSender<SubagentEvent>>,
}

impl DeleteTeamTool {
    pub fn new(team_manager: Arc<TeamManager>) -> Self {
        Self {
            team_manager,
            event_tx: None,
        }
    }

    pub fn with_event_sender(mut self, tx: mpsc::UnboundedSender<SubagentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }
}

#[async_trait::async_trait]
impl BaseTool for DeleteTeamTool {
    fn name(&self) -> &str {
        "TeamDelete"
    }

    fn description(&self) -> &str {
        "Shut down a team. Sends shutdown requests to all members via mailbox, \
         then cleans up team files. Running members will receive the shutdown \
         request on their next mailbox check."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "team_name": {
                    "type": "string",
                    "description": "Name of the team to delete."
                }
            },
            "required": ["team_name"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let team_name = match args.get("team_name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::fail("Missing required parameter: team_name"),
        };

        let team = match self.team_manager.get_team(team_name) {
            Some(t) => t,
            None => return ToolResult::fail(format!("Team '{team_name}' not found")),
        };

        // Warn if members are still busy — deleting prematurely loses results
        let busy_members: Vec<_> = team
            .members
            .iter()
            .filter(|m| m.status == TeamMemberStatus::Busy)
            .map(|m| m.name.as_str())
            .collect();
        if !busy_members.is_empty() {
            return ToolResult::fail(format!(
                "Cannot delete team '{team_name}' — {} member(s) still running: {}. \
                 Wait for all teammates to finish before deleting the team.",
                busy_members.len(),
                busy_members.join(", ")
            ));
        }

        let team_dir = self.team_manager.team_dir(team_name);

        // Send shutdown requests to all members
        for member in &team.members {
            let mailbox = Mailbox::new(&team_dir, &member.name);
            let shutdown_msg = MailboxMessage {
                id: uuid::Uuid::new_v4().to_string(),
                from: "leader".to_string(),
                content: "Team is being disbanded. Please wrap up and exit.".to_string(),
                timestamp_ms: opendev_runtime::now_ms(),
                read: false,
                msg_type: MessageType::ShutdownRequest,
            };
            let _ = mailbox.send(shutdown_msg);
        }

        // Clean up team files
        if let Err(e) = self.team_manager.delete_team(team_name) {
            return ToolResult::fail(format!("Failed to delete team: {e}"));
        }

        // Notify TUI
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(SubagentEvent::TeamDeleted {
                team_id: team_name.to_string(),
            });
        }

        info!(team = %team_name, "Team deleted");
        ToolResult::ok(format!(
            "Team '{team_name}' disbanded. Shutdown requests sent to {} members.",
            team.members.len()
        ))
    }
}
