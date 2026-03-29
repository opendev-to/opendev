use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolDisplayMeta, ToolResult};

/// Built-in subagent type definition.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AgentType {
    name: String,
    description: String,
    tools: Vec<String>,
}

/// Tool for listing available subagent types.
#[derive(Debug)]
pub struct AgentsTool;

/// Default agent types available in the system.
fn default_agent_types() -> Vec<AgentType> {
    vec![
        AgentType {
            name: "explore".into(),
            description: "Read-only agent for exploring and understanding codebases. \
                           Has access to file reading, search, and listing tools."
                .into(),
            tools: vec!["read_file".into(), "search".into(), "list_files".into()],
        },
        AgentType {
            name: "planner".into(),
            description: "Planning agent that creates implementation plans. \
                           Has read-only access to understand the codebase before planning."
                .into(),
            tools: vec![
                "read_file".into(),
                "search".into(),
                "list_files".into(),
                "write_file".into(),
            ],
        },
        AgentType {
            name: "ask_user".into(),
            description: "Agent that interacts with the user to gather information \
                           or clarify requirements."
                .into(),
            tools: vec!["ask_user".into()],
        },
    ]
}

#[async_trait::async_trait]
impl BaseTool for AgentsTool {
    fn name(&self) -> &str {
        "agents"
    }

    fn description(&self) -> &str {
        "List available subagent types with their descriptions and allowed tools."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform. Currently only 'list' is supported.",
                    "enum": ["list"]
                }
            }
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        match action {
            "list" => self.list_agents(ctx),
            other => ToolResult::fail(format!("Unknown action: {other}. Available actions: list")),
        }
    }

    fn display_meta(&self) -> Option<ToolDisplayMeta> {
        Some(ToolDisplayMeta {
            verb: "Agents",
            label: "agents",
            category: "Agent",
            primary_arg_keys: &["action"],
        })
    }
}

impl AgentsTool {
    fn list_agents(&self, ctx: &ToolContext) -> ToolResult {
        // Try to read agent types from context values (injected by runtime)
        let agents = if let Some(custom_agents) = ctx.values.get("agent_types") {
            match serde_json::from_value::<Vec<AgentType>>(custom_agents.clone()) {
                Ok(agents) => agents,
                Err(_) => default_agent_types(),
            }
        } else {
            default_agent_types()
        };

        if agents.is_empty() {
            return ToolResult::ok("No subagent types found.");
        }

        let mut parts = vec![format!("Available agents ({}):\n", agents.len())];

        for agent in &agents {
            parts.push(format!("  {}: {}", agent.name, agent.description));
            if !agent.tools.is_empty() {
                let tools_display: Vec<&str> =
                    agent.tools.iter().take(10).map(|s| s.as_str()).collect();
                parts.push(format!("    Tools: {}", tools_display.join(", ")));
            }
        }

        let output = parts.join("\n");

        let mut metadata = HashMap::new();
        metadata.insert(
            "agents".into(),
            serde_json::to_value(&agents).unwrap_or_default(),
        );
        metadata.insert("count".into(), serde_json::json!(agents.len()));

        ToolResult::ok_with_metadata(output, metadata)
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
