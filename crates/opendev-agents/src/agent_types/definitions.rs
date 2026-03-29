//! Agent definition: role + customizable configuration.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::roles::AgentRole;

/// Full definition of an agent's configuration.
///
/// Combines a role with customizable system prompt and tool access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// The agent's role.
    pub role: AgentRole,
    /// Custom system prompt (overrides the role default if set).
    pub system_prompt: Option<String>,
    /// Allowed tool names. Empty means all tools are available.
    pub tools: Vec<String>,
}

impl AgentDefinition {
    /// Create a new agent definition from a role with all defaults.
    pub fn from_role(role: AgentRole) -> Self {
        Self {
            role,
            system_prompt: None,
            tools: role.default_tools(),
        }
    }

    /// Get the effective system prompt (custom or role default).
    pub fn effective_system_prompt(&self) -> &str {
        self.system_prompt
            .as_deref()
            .unwrap_or_else(|| self.role.default_system_prompt())
    }

    /// Set a custom system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the tool allowlist.
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }

    /// Check if a tool is allowed for this agent.
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        self.tools.is_empty() || self.tools.iter().any(|t| t == tool_name)
    }

    /// Filter a set of tool schemas to only those allowed by this agent.
    pub fn filter_tool_schemas(&self, schemas: &[Value]) -> Vec<Value> {
        if self.tools.is_empty() {
            return schemas.to_vec();
        }
        schemas
            .iter()
            .filter(|schema| {
                let name = schema
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                self.is_tool_allowed(name)
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;
