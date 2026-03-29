use serde::{Deserialize, Serialize};

/// Classification of how an agent can be used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Main agent for top-level conversations.
    Primary,
    /// Can only be spawned as a subagent.
    #[default]
    Subagent,
    /// Can function in both primary and subagent roles.
    All,
}

impl AgentMode {
    pub(super) fn default_mode() -> Self {
        Self::default()
    }

    /// Parse a mode string, defaulting to `Subagent` for unknown values.
    pub fn parse_mode(s: &str) -> Self {
        match s {
            "primary" => Self::Primary,
            "all" => Self::All,
            _ => Self::Subagent,
        }
    }

    /// Whether this agent can be spawned as a subagent.
    pub fn can_be_subagent(&self) -> bool {
        matches!(self, Self::Subagent | Self::All)
    }

    /// Whether this agent can serve as a primary agent.
    pub fn can_be_primary(&self) -> bool {
        matches!(self, Self::Primary | Self::All)
    }
}

#[cfg(test)]
#[path = "mode_tests.rs"]
mod tests;
