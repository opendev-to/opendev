//! SubAgent specification types.
//!
//! Mirrors `opendev/core/agents/subagents/specs.py`.

use serde::{Deserialize, Serialize};

/// Specification for defining a subagent.
///
/// Subagents are ephemeral agents that handle isolated tasks.
/// They receive a task description, execute with their own context,
/// and return a single result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentSpec {
    /// Unique identifier for the subagent type.
    pub name: String,

    /// Human-readable description of what this subagent does.
    pub description: String,

    /// System prompt that defines the subagent's behavior and role.
    pub system_prompt: String,

    /// List of tool names this subagent has access to.
    /// If empty, inherits all tools from the main agent.
    #[serde(default)]
    pub tools: Vec<String>,

    /// Override model for this subagent.
    /// If None, uses the same model as the main agent.
    #[serde(default)]
    pub model: Option<String>,
}

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

    /// Check if this subagent has restricted tools.
    pub fn has_tool_restriction(&self) -> bool {
        !self.tools.is_empty()
    }
}

/// Built-in subagent definitions.
pub mod builtins {
    use super::SubAgentSpec;

    /// Tools available to the Code Explorer subagent.
    pub const CODE_EXPLORER_TOOLS: &[&str] = &["read_file", "search", "list_files"];

    /// Tools available to the Planner subagent.
    pub const PLANNER_TOOLS: &[&str] = &[
        "read_file",
        "search",
        "list_files",
        "write_file",
        "edit_file",
    ];

    /// Create the Code Explorer subagent spec.
    pub fn code_explorer(system_prompt: &str) -> SubAgentSpec {
        SubAgentSpec::new(
            "Code-Explorer",
            "Deep LOCAL codebase exploration and research. Systematically searches and \
             analyzes code to answer questions. USE FOR: Understanding code architecture, \
             finding patterns, researching implementation details in LOCAL files. \
             NOT FOR: External searches (GitHub repos, web) - use MCP tools or fetch_url instead.",
            system_prompt,
        )
        .with_tools(CODE_EXPLORER_TOOLS.iter().map(|s| s.to_string()).collect())
    }

    /// Create the Planner subagent spec.
    pub fn planner(system_prompt: &str) -> SubAgentSpec {
        SubAgentSpec::new(
            "Planner",
            "Codebase exploration and planning agent. Analyzes code, \
             understands patterns, identifies relevant files, and creates detailed \
             implementation plans. Writes the plan to a designated file path \
             provided in the prompt.",
            system_prompt,
        )
        .with_tools(PLANNER_TOOLS.iter().map(|s| s.to_string()).collect())
    }

    /// Create the Ask User subagent spec.
    pub fn ask_user(system_prompt: &str) -> SubAgentSpec {
        SubAgentSpec::new(
            "ask-user",
            "Ask the user clarifying questions with structured multiple-choice options. \
             Use when you need to gather preferences, clarify ambiguous requirements, \
             or confirm critical decisions.",
            system_prompt,
        )
        // No tools — UI-only interaction
    }

    /// Create the PR Reviewer subagent spec.
    pub fn pr_reviewer(system_prompt: &str) -> SubAgentSpec {
        SubAgentSpec::new(
            "PR-Reviewer",
            "Reviews pull request changes for code quality, bugs, and best practices.",
            system_prompt,
        )
        .with_tools(CODE_EXPLORER_TOOLS.iter().map(|s| s.to_string()).collect())
    }

    /// Create the Security Reviewer subagent spec.
    pub fn security_reviewer(system_prompt: &str) -> SubAgentSpec {
        SubAgentSpec::new(
            "Security-Reviewer",
            "Reviews code for security vulnerabilities and OWASP top 10 issues.",
            system_prompt,
        )
        .with_tools(CODE_EXPLORER_TOOLS.iter().map(|s| s.to_string()).collect())
    }

    /// Create the Web Clone subagent spec.
    pub fn web_clone(system_prompt: &str) -> SubAgentSpec {
        SubAgentSpec::new(
            "Web-Clone",
            "Clones a website's visual design by fetching and analyzing its HTML/CSS.",
            system_prompt,
        )
        .with_tools(
            vec![
                "read_file",
                "write_file",
                "edit_file",
                "list_files",
                "search",
                "web_fetch",
                "web_screenshot",
                "run_command",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        )
    }

    /// Tools available to the Project Init subagent.
    pub const PROJECT_INIT_TOOLS: &[&str] = &["read_file", "list_files", "search", "run_command"];

    /// Create the Project Init subagent spec.
    pub fn project_init(system_prompt: &str) -> SubAgentSpec {
        SubAgentSpec::new(
            "project_init",
            "Analyze codebase and generate project instructions",
            system_prompt,
        )
        .with_tools(PROJECT_INIT_TOOLS.iter().map(|s| s.to_string()).collect())
    }

    /// Create the Web Generator subagent spec.
    pub fn web_generator(system_prompt: &str) -> SubAgentSpec {
        SubAgentSpec::new(
            "Web-Generator",
            "Generates web applications from descriptions or screenshots.",
            system_prompt,
        )
        .with_tools(
            vec![
                "read_file",
                "write_file",
                "edit_file",
                "list_files",
                "search",
                "run_command",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_spec_new() {
        let spec = SubAgentSpec::new("test", "A test agent", "You are a test agent.");
        assert_eq!(spec.name, "test");
        assert!(!spec.has_tool_restriction());
        assert!(spec.model.is_none());
    }

    #[test]
    fn test_subagent_spec_with_tools() {
        let spec = SubAgentSpec::new("test", "desc", "prompt")
            .with_tools(vec!["read_file".into(), "search".into()]);
        assert!(spec.has_tool_restriction());
        assert_eq!(spec.tools.len(), 2);
    }

    #[test]
    fn test_subagent_spec_with_model() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_model("gpt-4");
        assert_eq!(spec.model.as_deref(), Some("gpt-4"));
    }

    #[test]
    fn test_subagent_spec_serde() {
        let spec = SubAgentSpec::new("test", "desc", "prompt")
            .with_tools(vec!["read_file".into()])
            .with_model("gpt-4");

        let json = serde_json::to_string(&spec).unwrap();
        let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "test");
        assert_eq!(restored.tools, vec!["read_file"]);
        assert_eq!(restored.model.as_deref(), Some("gpt-4"));
    }

    #[test]
    fn test_code_explorer_builtin() {
        let spec = builtins::code_explorer("You explore code.");
        assert_eq!(spec.name, "Code-Explorer");
        assert!(spec.has_tool_restriction());
        assert!(spec.tools.contains(&"read_file".to_string()));
        assert!(spec.tools.contains(&"search".to_string()));
        assert!(!spec.tools.contains(&"write_file".to_string()));
    }

    #[test]
    fn test_planner_builtin() {
        let spec = builtins::planner("You plan tasks.");
        assert_eq!(spec.name, "Planner");
        assert!(spec.tools.contains(&"write_file".to_string()));
        assert!(spec.tools.contains(&"edit_file".to_string()));
    }

    #[test]
    fn test_ask_user_builtin() {
        let spec = builtins::ask_user("You ask questions.");
        assert_eq!(spec.name, "ask-user");
        assert!(!spec.has_tool_restriction()); // No tools
    }

    #[test]
    fn test_pr_reviewer_builtin() {
        let spec = builtins::pr_reviewer("You review PRs.");
        assert_eq!(spec.name, "PR-Reviewer");
        assert!(spec.has_tool_restriction());
    }

    #[test]
    fn test_security_reviewer_builtin() {
        let spec = builtins::security_reviewer("You review security.");
        assert_eq!(spec.name, "Security-Reviewer");
        assert!(spec.has_tool_restriction());
    }

    #[test]
    fn test_web_clone_builtin() {
        let spec = builtins::web_clone("You clone websites.");
        assert_eq!(spec.name, "Web-Clone");
        assert!(spec.tools.contains(&"web_fetch".to_string()));
        assert!(spec.tools.contains(&"web_screenshot".to_string()));
    }

    #[test]
    fn test_web_generator_builtin() {
        let spec = builtins::web_generator("You generate web apps.");
        assert_eq!(spec.name, "Web-Generator");
        assert!(spec.tools.contains(&"write_file".to_string()));
    }

    #[test]
    fn test_project_init_builtin() {
        let spec = builtins::project_init("You analyze codebases.");
        assert_eq!(spec.name, "project_init");
        assert_eq!(
            spec.description,
            "Analyze codebase and generate project instructions"
        );
        assert!(spec.has_tool_restriction());
        assert_eq!(spec.tools.len(), 4);
        assert!(spec.tools.contains(&"read_file".to_string()));
        assert!(spec.tools.contains(&"list_files".to_string()));
        assert!(spec.tools.contains(&"search".to_string()));
        assert!(spec.tools.contains(&"run_command".to_string()));
        assert!(spec.model.is_none());
    }
}
