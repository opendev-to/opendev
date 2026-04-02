//! Tool for team members to read messages from their mailbox.

use std::collections::HashMap;
use std::sync::Arc;

use opendev_runtime::{Mailbox, MessageType, TeamManager};
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool that lets a team member check their inbox for new messages.
///
/// Reads all unread messages and marks them as read.
#[derive(Debug)]
pub struct CheckMailboxTool {
    team_manager: Arc<TeamManager>,
    /// The name of the agent that owns this mailbox instance.
    /// Injected when the tool is registered for a specific teammate.
    agent_name: String,
}

impl CheckMailboxTool {
    pub fn new(team_manager: Arc<TeamManager>, agent_name: impl Into<String>) -> Self {
        Self {
            team_manager,
            agent_name: agent_name.into(),
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for CheckMailboxTool {
    fn name(&self) -> &str {
        "CheckMailbox"
    }

    fn description(&self) -> &str {
        "Check your team mailbox for new messages from teammates or the team leader. \
         Returns all unread messages and marks them as read. Call periodically to \
         stay in sync with your team. Returns an empty list if no new messages."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
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
        // Resolve the team
        let teams = self.team_manager.list_teams();
        let team_name_filter = args.get("team_name").and_then(|v| v.as_str());
        let team = if let Some(name) = team_name_filter {
            teams.iter().find(|t| t.name == name)
        } else {
            teams.first()
        };

        let team = match team {
            Some(t) => t,
            None => {
                return ToolResult::ok(
                    "No active team. You are not currently part of a team.".to_string(),
                );
            }
        };

        let team_dir = self.team_manager.team_dir(&team.name);
        let mailbox = Mailbox::new(&team_dir, &self.agent_name);

        match mailbox.receive() {
            Ok(messages) if messages.is_empty() => {
                ToolResult::ok("No new messages in your mailbox.".to_string())
            }
            Ok(messages) => {
                let mut output = format!(
                    "{} new message(s) in your mailbox:\n\n",
                    messages.len()
                );
                for msg in &messages {
                    let type_label = match msg.msg_type {
                        MessageType::ShutdownRequest => "[SHUTDOWN REQUEST]",
                        MessageType::ShutdownResponse => "[SHUTDOWN RESPONSE]",
                        MessageType::Idle => "[IDLE NOTIFICATION]",
                        MessageType::Text => "",
                    };
                    output.push_str(&format!(
                        "--- From: {} {}\n{}\n\n",
                        msg.from, type_label, msg.content
                    ));
                }
                ToolResult::ok(output)
            }
            Err(e) => ToolResult::fail(format!("Failed to read mailbox: {e}")),
        }
    }
}
