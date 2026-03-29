//! Agent creator controller for the TUI.
//!
//! Manages form state for creating custom agent definitions, including
//! name, description, model, tools, and instructions.

/// Specification for a custom agent, produced by validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSpec {
    pub name: String,
    pub description: String,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub instructions: String,
}

/// Controller for the agent creation form.
pub struct AgentCreatorController {
    name: String,
    description: String,
    model: Option<String>,
    tools: Vec<String>,
    instructions: String,
}

impl AgentCreatorController {
    /// Create a new agent creator controller with empty fields.
    pub fn new() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            model: None,
            tools: Vec::new(),
            instructions: String::new(),
        }
    }

    /// Set the agent name.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Get the current agent name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set the agent description.
    pub fn set_description(&mut self, description: impl Into<String>) {
        self.description = description.into();
    }

    /// Get the current description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Set the model (or `None` to use the default).
    pub fn set_model(&mut self, model: Option<String>) {
        self.model = model;
    }

    /// Get the current model selection.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Add a tool to the agent's tool list.
    ///
    /// Duplicate tool names are silently ignored.
    pub fn add_tool(&mut self, tool: impl Into<String>) {
        let tool = tool.into();
        if !self.tools.contains(&tool) {
            self.tools.push(tool);
        }
    }

    /// Remove a tool by name. Returns `true` if the tool was present.
    pub fn remove_tool(&mut self, tool: &str) -> bool {
        if let Some(pos) = self.tools.iter().position(|t| t == tool) {
            self.tools.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get the current tool list.
    pub fn tools(&self) -> &[String] {
        &self.tools
    }

    /// Set the agent instructions (multi-line prompt text).
    pub fn set_instructions(&mut self, instructions: impl Into<String>) {
        self.instructions = instructions.into();
    }

    /// Get the current instructions.
    pub fn instructions(&self) -> &str {
        &self.instructions
    }

    /// Validate the current form state and produce an [`AgentSpec`].
    ///
    /// Returns an error string describing the first validation failure.
    pub fn validate(&self) -> Result<AgentSpec, String> {
        if self.name.trim().is_empty() {
            return Err("Agent name is required".into());
        }
        if self.description.trim().is_empty() {
            return Err("Agent description is required".into());
        }
        if self.instructions.trim().is_empty() {
            return Err("Agent instructions are required".into());
        }
        Ok(AgentSpec {
            name: self.name.trim().to_string(),
            description: self.description.trim().to_string(),
            model: self.model.clone(),
            tools: self.tools.clone(),
            instructions: self.instructions.clone(),
        })
    }

    /// Reset all fields to their default (empty) values.
    pub fn reset(&mut self) {
        self.name.clear();
        self.description.clear();
        self.model = None;
        self.tools.clear();
        self.instructions.clear();
    }
}

impl Default for AgentCreatorController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "agent_creator_tests.rs"]
mod tests;
