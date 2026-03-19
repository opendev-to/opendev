use super::types::SubAgentSpec;

/// Tools available to the Explore subagent.
pub const CODE_EXPLORER_TOOLS: &[&str] = &["read_file", "grep", "list_files", "run_command"];

/// Tools available to the Planner subagent.
pub const PLANNER_TOOLS: &[&str] = &["read_file", "grep", "list_files", "write_file", "edit_file"];

/// Create the Explore subagent spec.
pub fn code_explorer(system_prompt: &str) -> SubAgentSpec {
    SubAgentSpec::new(
        "Explore",
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

/// Tools available to the General subagent (broad access for multi-step tasks).
pub const GENERAL_TOOLS: &[&str] = &[
    "read_file",
    "grep",
    "list_files",
    "write_file",
    "edit_file",
    "multi_edit",
    "run_command",
    "web_fetch",
    "web_search",
    "patch",
    "git",
];

/// Create the General subagent spec.
///
/// This is the most versatile subagent type, with access to file operations,
/// shell commands, web tools, and git. Use for multi-step tasks that require
/// both reading and modifying code.
pub fn general(system_prompt: &str) -> SubAgentSpec {
    SubAgentSpec::new(
        "General",
        "Versatile multi-step agent for complex tasks requiring code reading, \
         editing, running commands, and web access. USE FOR: Implementing features, \
         fixing bugs, refactoring across multiple files, running tests, \
         and any task requiring broad tool access.",
        system_prompt,
    )
    .with_tools(GENERAL_TOOLS.iter().map(|s| s.to_string()).collect())
}

/// Tools available to the Build/Test subagent.
pub const BUILD_TOOLS: &[&str] = &[
    "read_file",
    "grep",
    "list_files",
    "run_command",
    "edit_file",
    "write_file",
];

/// Create the Build subagent spec.
///
/// Focused on building, testing, and fixing compilation/lint errors.
pub fn build(system_prompt: &str) -> SubAgentSpec {
    SubAgentSpec::new(
        "Build",
        "Build and test runner agent. Runs build commands, analyzes errors, \
         and fixes compilation or test failures. USE FOR: Running tests, \
         fixing build errors, CI failures, and lint warnings.",
        system_prompt,
    )
    .with_tools(BUILD_TOOLS.iter().map(|s| s.to_string()).collect())
}

/// Tools available to the Project Init subagent.
pub const PROJECT_INIT_TOOLS: &[&str] = &["read_file", "list_files", "grep", "run_command"];

/// Create the Project Init subagent spec.
pub fn project_init(system_prompt: &str) -> SubAgentSpec {
    SubAgentSpec::new(
        "project_init",
        "Analyze codebase and generate project instructions",
        system_prompt,
    )
    .with_tools(PROJECT_INIT_TOOLS.iter().map(|s| s.to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_explorer_builtin() {
        let spec = code_explorer("You explore code.");
        assert_eq!(spec.name, "Explore");
        assert!(spec.has_tool_restriction());
        assert!(spec.tools.contains(&"read_file".to_string()));
        assert!(spec.tools.contains(&"grep".to_string()));
        assert!(!spec.tools.contains(&"write_file".to_string()));
    }

    #[test]
    fn test_planner_builtin() {
        let spec = planner("You plan tasks.");
        assert_eq!(spec.name, "Planner");
        assert!(spec.tools.contains(&"write_file".to_string()));
        assert!(spec.tools.contains(&"edit_file".to_string()));
    }

    #[test]
    fn test_ask_user_builtin() {
        let spec = ask_user("You ask questions.");
        assert_eq!(spec.name, "ask-user");
        assert!(!spec.has_tool_restriction()); // No tools
    }

    #[test]
    fn test_general_builtin() {
        let spec = general("You are versatile.");
        assert_eq!(spec.name, "General");
        assert!(spec.has_tool_restriction());
        // General has broad tool access
        assert!(spec.tools.contains(&"read_file".to_string()));
        assert!(spec.tools.contains(&"write_file".to_string()));
        assert!(spec.tools.contains(&"edit_file".to_string()));
        assert!(spec.tools.contains(&"run_command".to_string()));
        assert!(spec.tools.contains(&"web_fetch".to_string()));
        assert!(spec.tools.contains(&"git".to_string()));
        assert_eq!(spec.tools.len(), GENERAL_TOOLS.len());
    }

    #[test]
    fn test_build_builtin() {
        let spec = build("You build code.");
        assert_eq!(spec.name, "Build");
        assert!(spec.has_tool_restriction());
        assert!(spec.tools.contains(&"run_command".to_string()));
        assert!(spec.tools.contains(&"edit_file".to_string()));
        assert!(spec.tools.contains(&"read_file".to_string()));
        assert_eq!(spec.tools.len(), BUILD_TOOLS.len());
    }

    #[test]
    fn test_project_init_builtin() {
        let spec = project_init("You analyze codebases.");
        assert_eq!(spec.name, "project_init");
        assert_eq!(
            spec.description,
            "Analyze codebase and generate project instructions"
        );
        assert!(spec.has_tool_restriction());
        assert_eq!(spec.tools.len(), 4);
        assert!(spec.tools.contains(&"read_file".to_string()));
        assert!(spec.tools.contains(&"list_files".to_string()));
        assert!(spec.tools.contains(&"grep".to_string()));
        assert!(spec.tools.contains(&"run_command".to_string()));
        assert!(spec.model.is_none());
    }
}
