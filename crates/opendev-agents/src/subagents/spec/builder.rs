use std::collections::HashMap;

use super::mode::AgentMode;
use super::permissions::{PermissionAction, PermissionRule, glob_match, pattern_specificity};
use super::types::{AgentPermissionMode, IsolationMode, SubAgentSpec};

impl SubAgentSpec {
    /// Create a new subagent spec.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        system_prompt: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            system_prompt: system_prompt.into(),
            tools: Vec::new(),
            model: None,
            max_steps: None,
            hidden: false,
            temperature: None,
            top_p: None,
            mode: AgentMode::Subagent,
            max_tokens: None,
            color: None,
            permission: HashMap::new(),
            disable: false,
            permission_mode: Default::default(),
            isolation: Default::default(),
            background: false,
            omit_instructions: false,
        }
    }

    /// Set the tools available to this subagent.
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }

    /// Set an override model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the maximum number of iterations.
    pub fn with_max_steps(mut self, steps: u32) -> Self {
        self.max_steps = Some(steps);
        self
    }

    /// Mark this agent as hidden from UI.
    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// Set an override temperature.
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set an override top_p.
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// Set the agent mode.
    pub fn with_mode(mut self, mode: AgentMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set an override max_tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set the display color (hex string like `"#38A3EE"`).
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Set the permission rules for this subagent.
    pub fn with_permission(mut self, permission: HashMap<String, PermissionRule>) -> Self {
        self.permission = permission;
        self
    }

    /// Mark this agent as disabled.
    pub fn with_disable(mut self, disable: bool) -> Self {
        self.disable = disable;
        self
    }

    /// Set the permission mode override.
    pub fn with_permission_mode(mut self, mode: AgentPermissionMode) -> Self {
        self.permission_mode = mode;
        self
    }

    /// Set the isolation strategy.
    pub fn with_isolation(mut self, isolation: IsolationMode) -> Self {
        self.isolation = isolation;
        self
    }

    /// Auto-background this agent when spawned.
    pub fn with_background(mut self, background: bool) -> Self {
        self.background = background;
        self
    }

    /// Omit project instructions from system prompt.
    pub fn with_omit_instructions(mut self, omit: bool) -> Self {
        self.omit_instructions = omit;
        self
    }

    /// Check if this subagent has restricted tools.
    pub fn has_tool_restriction(&self) -> bool {
        !self.tools.is_empty()
    }

    /// Evaluate whether a tool call is permitted by this agent's permission rules.
    ///
    /// Returns the action for the given tool name and argument pattern.
    /// If no matching rule is found, returns `None` (caller decides default).
    ///
    /// More specific patterns take precedence over wildcards.
    /// Within the same specificity level, the last-inserted rule wins.
    pub fn evaluate_permission(
        &self,
        tool_name: &str,
        arg_pattern: &str,
    ) -> Option<PermissionAction> {
        if self.permission.is_empty() {
            return None;
        }

        // Find the most specific matching rule.
        // Specificity: exact match > partial glob > wildcard "*"
        let mut best_match: Option<(PermissionAction, usize)> = None; // (action, specificity)

        for (tool_pattern, rule) in &self.permission {
            if !glob_match(tool_pattern, tool_name) {
                continue;
            }
            match rule {
                PermissionRule::Action(action) => {
                    let specificity = pattern_specificity(tool_pattern);
                    if best_match.as_ref().is_none_or(|(_, s)| specificity >= *s) {
                        best_match = Some((*action, specificity));
                    }
                }
                PermissionRule::Patterns(patterns) => {
                    for (pattern, action) in patterns {
                        if glob_match(pattern, arg_pattern) {
                            let specificity = pattern_specificity(pattern);
                            if best_match.as_ref().is_none_or(|(_, s)| specificity >= *s) {
                                best_match = Some((*action, specificity));
                            }
                        }
                    }
                }
            }
        }

        best_match.map(|(action, _)| action)
    }

    /// Check which tools should be completely disabled (removed from LLM schema).
    ///
    /// A tool is disabled if its last matching rule is a blanket `"deny"` action
    /// (either `PermissionRule::Action(Deny)` or a patterns map with only `"*": "deny"`).
    pub fn disabled_tools(&self, tool_names: &[&str]) -> Vec<String> {
        let mut disabled = Vec::new();
        for &tool in tool_names {
            let is_blanket_deny = self.permission.iter().any(|(tp, rule)| {
                glob_match(tp, tool)
                    && match rule {
                        PermissionRule::Action(PermissionAction::Deny) => true,
                        PermissionRule::Patterns(p) => {
                            p.len() == 1 && p.get("*") == Some(&PermissionAction::Deny)
                        }
                        _ => false,
                    }
            });
            if is_blanket_deny {
                disabled.push(tool.to_string());
            }
        }
        disabled
    }
}

#[cfg(test)]
#[path = "builder_tests.rs"]
mod tests;
