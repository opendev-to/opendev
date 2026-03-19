//! Predefined agent roles for common development tasks.

use serde::{Deserialize, Serialize};

/// Predefined agent roles for common development tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentRole {
    /// General-purpose coding agent with full tool access.
    Code,
    /// Planning agent focused on architecture and task decomposition.
    Plan,
    /// Testing agent specialized in writing and running tests.
    Test,
    /// Build agent for compilation, linting, and CI tasks.
    Build,
}

impl AgentRole {
    /// Return the default system prompt snippet for this role.
    pub fn default_system_prompt(&self) -> &'static str {
        match self {
            AgentRole::Code => {
                "You are a coding agent. Your primary job is to read, write, and edit \
                 source code. Use tools to explore the codebase, make targeted edits, \
                 and verify your changes compile. Focus on correctness and minimal diffs."
            }
            AgentRole::Plan => {
                "You are a planning agent. Analyze the user's request and break it into \
                 concrete, ordered steps. Identify files to change, dependencies between \
                 tasks, and potential risks. Do NOT make code changes yourself — produce \
                 a structured plan for execution agents."
            }
            AgentRole::Test => {
                "You are a testing agent. Your job is to write, run, and verify tests. \
                 Read the relevant source code, write comprehensive tests covering edge \
                 cases, run them, and report results. Fix any failing tests you introduce."
            }
            AgentRole::Build => {
                "You are a build agent. Your job is to compile the project, run linters, \
                 and fix any build or lint errors. Focus on making the project build \
                 cleanly with zero warnings."
            }
        }
    }

    /// Return the default tool allowlist for this role.
    ///
    /// An empty vec means "all tools" (no restriction).
    pub fn default_tools(&self) -> Vec<String> {
        match self {
            AgentRole::Code => vec![], // all tools
            AgentRole::Plan => vec![
                "read_file".into(),
                "list_files".into(),
                "grep".into(),
                "find_symbol".into(),
                "find_referencing_symbols".into(),
                "web_search".into(),
                "task_complete".into(),
            ],
            AgentRole::Test => vec![
                "read_file".into(),
                "write_file".into(),
                "edit_file".into(),
                "list_files".into(),
                "grep".into(),
                "bash".into(),
                "task_complete".into(),
            ],
            AgentRole::Build => vec![
                "read_file".into(),
                "edit_file".into(),
                "bash".into(),
                "list_files".into(),
                "grep".into(),
                "task_complete".into(),
            ],
        }
    }
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentRole::Code => write!(f, "Code"),
            AgentRole::Plan => write!(f, "Plan"),
            AgentRole::Test => write!(f, "Test"),
            AgentRole::Build => write!(f, "Build"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_role_display() {
        assert_eq!(AgentRole::Code.to_string(), "Code");
        assert_eq!(AgentRole::Plan.to_string(), "Plan");
        assert_eq!(AgentRole::Test.to_string(), "Test");
        assert_eq!(AgentRole::Build.to_string(), "Build");
    }

    #[test]
    fn test_agent_role_default_system_prompt() {
        assert!(
            AgentRole::Code
                .default_system_prompt()
                .contains("coding agent")
        );
        assert!(
            AgentRole::Plan
                .default_system_prompt()
                .contains("planning agent")
        );
        assert!(
            AgentRole::Test
                .default_system_prompt()
                .contains("testing agent")
        );
        assert!(
            AgentRole::Build
                .default_system_prompt()
                .contains("build agent")
        );
    }

    #[test]
    fn test_agent_role_default_tools() {
        assert!(AgentRole::Code.default_tools().is_empty());
        let plan_tools = AgentRole::Plan.default_tools();
        assert!(plan_tools.contains(&"read_file".to_string()));
        assert!(!plan_tools.contains(&"bash".to_string()));
        assert!(
            AgentRole::Test
                .default_tools()
                .contains(&"bash".to_string())
        );
        assert!(
            AgentRole::Build
                .default_tools()
                .contains(&"bash".to_string())
        );
    }
}
