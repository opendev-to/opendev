//! Agent definition: role + customizable configuration.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use opendev_runtime::ThinkingLevel;

use super::roles::AgentRole;

/// Full definition of an agent's configuration.
///
/// Combines a role with customizable system prompt, thinking level,
/// tool access, and optional per-agent model overrides for the
/// thinking and critique phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// The agent's role.
    pub role: AgentRole,
    /// Custom system prompt (overrides the role default if set).
    pub system_prompt: Option<String>,
    /// Thinking level (overrides the role default if set).
    pub thinking_level: Option<ThinkingLevel>,
    /// Allowed tool names. Empty means all tools are available.
    pub tools: Vec<String>,
    /// Optional model override for the thinking phase.
    pub thinking_model: Option<String>,
    /// Optional model override for the critique phase.
    pub critique_model: Option<String>,
}

impl AgentDefinition {
    /// Create a new agent definition from a role with all defaults.
    pub fn from_role(role: AgentRole) -> Self {
        Self {
            role,
            system_prompt: None,
            thinking_level: None,
            tools: role.default_tools(),
            thinking_model: None,
            critique_model: None,
        }
    }

    /// Get the effective system prompt (custom or role default).
    pub fn effective_system_prompt(&self) -> &str {
        self.system_prompt
            .as_deref()
            .unwrap_or_else(|| self.role.default_system_prompt())
    }

    /// Get the effective thinking level (custom or role default).
    pub fn effective_thinking_level(&self) -> ThinkingLevel {
        self.thinking_level
            .unwrap_or_else(|| self.role.default_thinking_level())
    }

    /// Set a custom system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set a custom thinking level.
    pub fn with_thinking_level(mut self, level: ThinkingLevel) -> Self {
        self.thinking_level = Some(level);
        self
    }

    /// Set the tool allowlist.
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }

    /// Set the thinking model override.
    pub fn with_thinking_model(mut self, model: impl Into<String>) -> Self {
        self.thinking_model = Some(model.into());
        self
    }

    /// Set the critique model override.
    pub fn with_critique_model(mut self, model: impl Into<String>) -> Self {
        self.critique_model = Some(model.into());
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
mod tests {
    use super::*;

    #[test]
    fn test_agent_definition_from_role() {
        let def = AgentDefinition::from_role(AgentRole::Code);
        assert_eq!(def.role, AgentRole::Code);
        assert!(def.system_prompt.is_none());
        assert!(def.thinking_level.is_none());
        assert!(def.thinking_model.is_none());
        assert!(def.critique_model.is_none());
    }

    #[test]
    fn test_agent_definition_effective_system_prompt() {
        let def = AgentDefinition::from_role(AgentRole::Code);
        assert!(def.effective_system_prompt().contains("coding agent"));
        let def = def.with_system_prompt("Custom prompt");
        assert_eq!(def.effective_system_prompt(), "Custom prompt");
    }

    #[test]
    fn test_agent_definition_effective_thinking_level() {
        let def = AgentDefinition::from_role(AgentRole::Plan);
        assert_eq!(def.effective_thinking_level(), ThinkingLevel::High);
        let def = def.with_thinking_level(ThinkingLevel::Off);
        assert_eq!(def.effective_thinking_level(), ThinkingLevel::Off);
    }

    #[test]
    fn test_agent_definition_with_models() {
        let def = AgentDefinition::from_role(AgentRole::Code)
            .with_thinking_model("gpt-4o")
            .with_critique_model("claude-3-haiku");
        assert_eq!(def.thinking_model.as_deref(), Some("gpt-4o"));
        assert_eq!(def.critique_model.as_deref(), Some("claude-3-haiku"));
    }

    #[test]
    fn test_agent_definition_is_tool_allowed() {
        let code = AgentDefinition::from_role(AgentRole::Code);
        assert!(code.is_tool_allowed("bash"));
        assert!(code.is_tool_allowed("anything"));
        let plan = AgentDefinition::from_role(AgentRole::Plan);
        assert!(plan.is_tool_allowed("read_file"));
        assert!(!plan.is_tool_allowed("bash"));
    }

    #[test]
    fn test_agent_definition_filter_tool_schemas() {
        let schemas = vec![
            serde_json::json!({"function": {"name": "read_file"}}),
            serde_json::json!({"function": {"name": "bash"}}),
            serde_json::json!({"function": {"name": "search"}}),
        ];
        let code = AgentDefinition::from_role(AgentRole::Code);
        assert_eq!(code.filter_tool_schemas(&schemas).len(), 3);
        let plan = AgentDefinition::from_role(AgentRole::Plan);
        let filtered = plan.filter_tool_schemas(&schemas);
        assert_eq!(filtered.len(), 2);
        assert!(
            filtered
                .iter()
                .all(|s| s["function"]["name"].as_str().unwrap() != "bash")
        );
    }

    #[test]
    fn test_agent_definition_with_tools() {
        let def = AgentDefinition::from_role(AgentRole::Code)
            .with_tools(vec!["read_file".into(), "bash".into()]);
        assert!(def.is_tool_allowed("read_file"));
        assert!(def.is_tool_allowed("bash"));
        assert!(!def.is_tool_allowed("write_file"));
    }

    #[test]
    fn test_agent_definition_serialization() {
        let def = AgentDefinition::from_role(AgentRole::Test)
            .with_thinking_model("gpt-4o")
            .with_critique_model("claude-3-haiku");
        let json = serde_json::to_string(&def).unwrap();
        let roundtrip: AgentDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.role, AgentRole::Test);
        assert_eq!(roundtrip.thinking_model.as_deref(), Some("gpt-4o"));
        assert_eq!(roundtrip.critique_model.as_deref(), Some("claude-3-haiku"));
    }
}
