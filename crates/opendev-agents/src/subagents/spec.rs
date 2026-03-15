//! SubAgent specification types.
//!
//! Mirrors `opendev/core/agents/subagents/specs.py`.

use std::collections::HashMap;

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

    /// Maximum number of ReAct loop iterations for this subagent.
    /// If None, uses the default limit (25).
    #[serde(default)]
    pub max_steps: Option<u32>,

    /// Whether this agent is hidden from UI/menu selection.
    /// Hidden agents (like internal compaction agents) are not shown
    /// in the agent list but can still be spawned programmatically.
    #[serde(default)]
    pub hidden: bool,

    /// Override temperature for this subagent.
    /// If None, uses the default (0.7).
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Override top_p (nucleus sampling) for this subagent.
    /// If None, uses the provider default.
    #[serde(default)]
    pub top_p: Option<f32>,

    /// Agent mode classification.
    /// - `primary`: Main agents that handle top-level conversations.
    /// - `subagent`: Can only be spawned via spawn_subagent tool.
    /// - `all`: Can function in both primary and subagent roles.
    #[serde(default = "AgentMode::default_mode")]
    pub mode: AgentMode,

    /// Override max_tokens for this subagent's LLM calls.
    /// If None, uses the default (4096).
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Optional hex color for TUI display (e.g., `"#38A3EE"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    /// Per-tool permission rules.
    ///
    /// Maps tool names to permission rules. Each rule can be:
    /// - A single action string (`"allow"`, `"deny"`, `"ask"`)
    /// - A map of glob patterns to actions (`{ "git *": "allow", "rm *": "deny" }`)
    ///
    /// Tool names support wildcards (`"*"` = all tools, `"read_*"` = all read tools).
    /// Last matching rule wins when multiple patterns match.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub permission: HashMap<String, PermissionRule>,

    /// Whether this agent is disabled (not available for use).
    #[serde(default)]
    pub disable: bool,

    /// Whether to skip appending project instruction files (CLAUDE.md, etc.)
    /// to this subagent's system prompt. Useful for exploration agents that
    /// should discover code structure from source, not from documentation.
    #[serde(default)]
    pub skip_project_instructions: bool,
}

/// Action to take when a permission rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    /// Allow the tool call without user approval.
    Allow,
    /// Deny the tool call entirely.
    Deny,
    /// Prompt the user for approval.
    Ask,
}

/// A permission rule for a tool — either a blanket action or pattern-specific.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionRule {
    /// Single action applies to all patterns for this tool.
    Action(PermissionAction),
    /// Map of glob patterns to actions.
    /// Example: `{ "*": "ask", "git *": "allow", "rm -rf *": "deny" }`
    Patterns(HashMap<String, PermissionAction>),
}

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
    fn default_mode() -> Self {
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
            skip_project_instructions: false,
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

    /// Skip appending project instruction files to the system prompt.
    pub fn with_skip_project_instructions(mut self, skip: bool) -> Self {
        self.skip_project_instructions = skip;
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

/// Compute specificity of a glob pattern (higher = more specific).
///
/// `"*"` → 0, `"git *"` → 4, `"git status"` → 10 (exact match).
/// Patterns with fewer wildcards and more literal characters are more specific.
pub(crate) fn pattern_specificity(pattern: &str) -> usize {
    if pattern == "*" {
        return 0;
    }
    // Count non-wildcard characters as specificity score.
    pattern.chars().filter(|c| *c != '*' && *c != '?').count()
}

/// Simple glob-style matching: `*` matches any sequence, `?` matches one char.
///
/// Matching is case-sensitive and operates on the full string.
pub(crate) fn glob_match(pattern: &str, input: &str) -> bool {
    let pattern = pattern.as_bytes();
    let input = input.as_bytes();
    let mut pi = 0;
    let mut ii = 0;
    let mut star_pi = usize::MAX;
    let mut star_ii = 0;

    while ii < input.len() {
        if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == input[ii]) {
            pi += 1;
            ii += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star_pi = pi;
            star_ii = ii;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ii += 1;
            ii = star_ii;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

/// Built-in subagent definitions.
pub mod builtins {
    use super::SubAgentSpec;

    /// Tools available to the Code Explorer subagent.
    pub const CODE_EXPLORER_TOOLS: &[&str] = &[
        "read_file",
        "search",
        "list_files",
        "find_symbol",
        "find_referencing_symbols",
        "run_command",
    ];

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
        .with_skip_project_instructions(true)
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
        "search",
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
        "search",
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
    fn test_general_builtin() {
        let spec = builtins::general("You are versatile.");
        assert_eq!(spec.name, "General");
        assert!(spec.has_tool_restriction());
        // General has broad tool access
        assert!(spec.tools.contains(&"read_file".to_string()));
        assert!(spec.tools.contains(&"write_file".to_string()));
        assert!(spec.tools.contains(&"edit_file".to_string()));
        assert!(spec.tools.contains(&"run_command".to_string()));
        assert!(spec.tools.contains(&"web_fetch".to_string()));
        assert!(spec.tools.contains(&"git".to_string()));
        assert_eq!(spec.tools.len(), builtins::GENERAL_TOOLS.len());
    }

    #[test]
    fn test_build_builtin() {
        let spec = builtins::build("You build code.");
        assert_eq!(spec.name, "Build");
        assert!(spec.has_tool_restriction());
        assert!(spec.tools.contains(&"run_command".to_string()));
        assert!(spec.tools.contains(&"edit_file".to_string()));
        assert!(spec.tools.contains(&"read_file".to_string()));
        assert_eq!(spec.tools.len(), builtins::BUILD_TOOLS.len());
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

    // ---- New extended fields ----

    #[test]
    fn test_subagent_spec_defaults() {
        let spec = SubAgentSpec::new("test", "desc", "prompt");
        assert!(spec.max_steps.is_none());
        assert!(!spec.hidden);
        assert!(spec.temperature.is_none());
        assert!(spec.top_p.is_none());
        assert_eq!(spec.mode, AgentMode::Subagent);
    }

    #[test]
    fn test_subagent_spec_with_max_steps() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_max_steps(50);
        assert_eq!(spec.max_steps, Some(50));
    }

    #[test]
    fn test_subagent_spec_with_hidden() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_hidden(true);
        assert!(spec.hidden);
    }

    #[test]
    fn test_subagent_spec_with_temperature() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_temperature(0.3);
        assert_eq!(spec.temperature, Some(0.3));
    }

    #[test]
    fn test_subagent_spec_serde_extended_fields() {
        let spec = SubAgentSpec::new("test", "desc", "prompt")
            .with_max_steps(50)
            .with_hidden(true)
            .with_temperature(0.5);

        let json = serde_json::to_string(&spec).unwrap();
        let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.max_steps, Some(50));
        assert!(restored.hidden);
        assert_eq!(restored.temperature, Some(0.5));
    }

    // ---- Agent mode ----

    #[test]
    fn test_agent_mode_default() {
        assert_eq!(AgentMode::default(), AgentMode::Subagent);
    }

    #[test]
    fn test_agent_mode_from_str() {
        assert_eq!(AgentMode::parse_mode("primary"), AgentMode::Primary);
        assert_eq!(AgentMode::parse_mode("subagent"), AgentMode::Subagent);
        assert_eq!(AgentMode::parse_mode("all"), AgentMode::All);
        assert_eq!(AgentMode::parse_mode("unknown"), AgentMode::Subagent);
    }

    #[test]
    fn test_agent_mode_capabilities() {
        assert!(AgentMode::Primary.can_be_primary());
        assert!(!AgentMode::Primary.can_be_subagent());

        assert!(!AgentMode::Subagent.can_be_primary());
        assert!(AgentMode::Subagent.can_be_subagent());

        assert!(AgentMode::All.can_be_primary());
        assert!(AgentMode::All.can_be_subagent());
    }

    #[test]
    fn test_agent_mode_serde() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_mode(AgentMode::Primary);

        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"primary\""));

        let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.mode, AgentMode::Primary);
    }

    #[test]
    fn test_with_top_p() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_top_p(0.9);
        assert_eq!(spec.top_p, Some(0.9));
    }

    #[test]
    fn test_top_p_serde() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_top_p(0.95);
        let json = serde_json::to_string(&spec).unwrap();
        let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.top_p, Some(0.95));
    }

    // ---- Color field ----

    #[test]
    fn test_with_color() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_color("#38A3EE");
        assert_eq!(spec.color.as_deref(), Some("#38A3EE"));
    }

    #[test]
    fn test_color_serde() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_color("#FF0000");
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("#FF0000"));
        let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.color.as_deref(), Some("#FF0000"));
    }

    #[test]
    fn test_color_skipped_when_none() {
        let spec = SubAgentSpec::new("test", "desc", "prompt");
        assert!(spec.color.is_none());
        let json = serde_json::to_string(&spec).unwrap();
        assert!(!json.contains("color"));
    }

    // ---- max_tokens field ----

    #[test]
    fn test_with_max_tokens() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_max_tokens(8192);
        assert_eq!(spec.max_tokens, Some(8192));
    }

    #[test]
    fn test_max_tokens_default_none() {
        let spec = SubAgentSpec::new("test", "desc", "prompt");
        assert!(spec.max_tokens.is_none());
    }

    #[test]
    fn test_max_tokens_serde() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_max_tokens(16384);
        let json = serde_json::to_string(&spec).unwrap();
        let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.max_tokens, Some(16384));
    }

    // ---- Permission system ----

    #[test]
    fn test_glob_match_basic() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("read_*", "read_file"));
        assert!(glob_match("read_*", "read_dir"));
        assert!(!glob_match("read_*", "write_file"));
        assert!(glob_match("?at", "cat"));
        assert!(!glob_match("?at", "chat"));
        assert!(glob_match("git *", "git status"));
        assert!(glob_match("git *", "git push origin main"));
    }

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("bash", "bash"));
        assert!(!glob_match("bash", "bash2"));
        assert!(!glob_match("bash2", "bash"));
    }

    #[test]
    fn test_permission_action_serde() {
        let action = PermissionAction::Allow;
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, "\"allow\"");
        let restored: PermissionAction = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, PermissionAction::Allow);
    }

    #[test]
    fn test_permission_rule_single_action() {
        let rule: PermissionRule = serde_json::from_str("\"deny\"").unwrap();
        assert!(matches!(
            rule,
            PermissionRule::Action(PermissionAction::Deny)
        ));
    }

    #[test]
    fn test_permission_rule_patterns() {
        let json = r#"{"*": "ask", "git *": "allow", "rm -rf *": "deny"}"#;
        let rule: PermissionRule = serde_json::from_str(json).unwrap();
        if let PermissionRule::Patterns(p) = &rule {
            assert_eq!(p.len(), 3);
            assert_eq!(p["*"], PermissionAction::Ask);
            assert_eq!(p["git *"], PermissionAction::Allow);
            assert_eq!(p["rm -rf *"], PermissionAction::Deny);
        } else {
            panic!("Expected Patterns variant");
        }
    }

    #[test]
    fn test_evaluate_permission_blanket_action() {
        let mut perms = HashMap::new();
        perms.insert(
            "bash".to_string(),
            PermissionRule::Action(PermissionAction::Deny),
        );

        let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

        assert_eq!(
            spec.evaluate_permission("bash", "anything"),
            Some(PermissionAction::Deny)
        );
        assert_eq!(
            spec.evaluate_permission("read_file", "anything"),
            None // No rule for read_file
        );
    }

    #[test]
    fn test_evaluate_permission_wildcard_tool() {
        let mut perms = HashMap::new();
        perms.insert(
            "*".to_string(),
            PermissionRule::Action(PermissionAction::Ask),
        );

        let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

        assert_eq!(
            spec.evaluate_permission("bash", "anything"),
            Some(PermissionAction::Ask)
        );
        assert_eq!(
            spec.evaluate_permission("read_file", "anything"),
            Some(PermissionAction::Ask)
        );
    }

    #[test]
    fn test_evaluate_permission_pattern_matching() {
        let mut patterns = HashMap::new();
        patterns.insert("*".to_string(), PermissionAction::Ask);
        patterns.insert("git *".to_string(), PermissionAction::Allow);
        patterns.insert("rm -rf *".to_string(), PermissionAction::Deny);

        let mut perms = HashMap::new();
        perms.insert("bash".to_string(), PermissionRule::Patterns(patterns));

        let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

        assert_eq!(
            spec.evaluate_permission("bash", "git status"),
            Some(PermissionAction::Allow)
        );
        assert_eq!(
            spec.evaluate_permission("bash", "rm -rf /"),
            Some(PermissionAction::Deny)
        );
        assert_eq!(
            spec.evaluate_permission("bash", "npm install"),
            Some(PermissionAction::Ask)
        );
    }

    #[test]
    fn test_evaluate_permission_no_rules() {
        let spec = SubAgentSpec::new("test", "desc", "prompt");
        assert_eq!(spec.evaluate_permission("bash", "anything"), None);
    }

    #[test]
    fn test_disabled_tools_blanket_deny() {
        let mut perms = HashMap::new();
        perms.insert(
            "edit".to_string(),
            PermissionRule::Action(PermissionAction::Deny),
        );
        perms.insert(
            "bash".to_string(),
            PermissionRule::Action(PermissionAction::Allow),
        );

        let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

        let disabled = spec.disabled_tools(&["edit", "bash", "read_file"]);
        assert_eq!(disabled, vec!["edit"]);
    }

    #[test]
    fn test_disabled_tools_pattern_deny_not_blanket() {
        // Pattern-specific deny should NOT disable the tool entirely.
        let mut patterns = HashMap::new();
        patterns.insert("rm *".to_string(), PermissionAction::Deny);
        patterns.insert("*".to_string(), PermissionAction::Allow);

        let mut perms = HashMap::new();
        perms.insert("bash".to_string(), PermissionRule::Patterns(patterns));

        let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

        let disabled = spec.disabled_tools(&["bash"]);
        assert!(
            disabled.is_empty(),
            "Pattern-specific deny should not disable tool"
        );
    }

    #[test]
    fn test_disable_field() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_disable(true);
        assert!(spec.disable);

        let spec2 = SubAgentSpec::new("test", "desc", "prompt");
        assert!(!spec2.disable);
    }

    #[test]
    fn test_disable_serde() {
        let spec = SubAgentSpec::new("test", "desc", "prompt").with_disable(true);
        let json = serde_json::to_string(&spec).unwrap();
        let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
        assert!(restored.disable);
    }

    #[test]
    fn test_permission_serde_roundtrip() {
        let mut patterns = HashMap::new();
        patterns.insert("*".to_string(), PermissionAction::Ask);
        patterns.insert("git *".to_string(), PermissionAction::Allow);

        let mut perms = HashMap::new();
        perms.insert("bash".to_string(), PermissionRule::Patterns(patterns));
        perms.insert(
            "edit".to_string(),
            PermissionRule::Action(PermissionAction::Deny),
        );

        let spec = SubAgentSpec::new("test", "desc", "prompt").with_permission(perms);

        let json = serde_json::to_string(&spec).unwrap();
        let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();

        assert_eq!(
            restored.evaluate_permission("bash", "git status"),
            Some(PermissionAction::Allow)
        );
        assert_eq!(
            restored.evaluate_permission("edit", "any_file"),
            Some(PermissionAction::Deny)
        );
    }

    #[test]
    fn test_permission_skipped_when_empty() {
        let spec = SubAgentSpec::new("test", "desc", "prompt");
        let json = serde_json::to_string(&spec).unwrap();
        assert!(!json.contains("permission"));
    }
}
