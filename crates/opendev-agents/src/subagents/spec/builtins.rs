use super::types::SubAgentSpec;

/// Tools available to the Explore subagent.
pub const CODE_EXPLORER_TOOLS: &[&str] = &["Read", "Grep", "Glob", "Bash", "ast_grep"];

/// Tools available to the Planner subagent.
pub const PLANNER_TOOLS: &[&str] = &["Read", "Grep", "Glob", "Write", "Edit"];

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

/// Create the General subagent spec.
///
/// This is the most versatile subagent type with access to all parent tools.
/// Use for multi-step tasks that require broad tool access.
pub fn general(system_prompt: &str) -> SubAgentSpec {
    SubAgentSpec::new(
        "General",
        "Versatile multi-step agent for complex tasks requiring code reading, \
         editing, running commands, and web access. USE FOR: Implementing features, \
         fixing bugs, refactoring across multiple files, running tests, \
         and any task requiring broad tool access.",
        system_prompt,
    )
    // No .with_tools() — inherits all parent tools
}

/// Tools available to the Build/Test subagent.
pub const BUILD_TOOLS: &[&str] = &["Read", "Grep", "Glob", "Bash", "Edit", "Write"];

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

/// Tools available to the Verification subagent (read-only).
pub const VERIFICATION_TOOLS: &[&str] = &["Read", "Grep", "Glob", "Bash"];

/// Create the Verification subagent spec.
///
/// Adversarial code review agent that runs in background to find bugs,
/// edge cases, and regressions in recent changes.
pub fn verification(system_prompt: &str) -> SubAgentSpec {
    SubAgentSpec::new(
        "Verification",
        "Adversarial code review agent. Finds bugs, edge cases, and regressions \
         in recent changes. Always runs in background. USE FOR: After making 3+ file \
         edits, backend/API changes, or infrastructure changes, spawn this agent \
         to independently verify your work.",
        system_prompt,
    )
    .with_tools(VERIFICATION_TOOLS.iter().map(|s| s.to_string()).collect())
    .with_background(true)
}

/// Tools available to the Project Init subagent.
pub const PROJECT_INIT_TOOLS: &[&str] = &["Read", "Glob", "Grep", "Bash"];

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
#[path = "builtins_tests.rs"]
mod tests;
