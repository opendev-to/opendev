//! Subagent manager for registering and executing subagents.
//!
//! Manages a collection of subagent specifications and provides
//! lookup by name or type. Also provides the execution entry point
//! for spawning subagents with isolated ReAct loops.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::spec::SubAgentSpec;
use crate::react_loop::{ReactLoop, ReactLoopConfig};
use crate::traits::{AgentError, AgentResult, TaskMonitor};
use opendev_http::adapted_client::AdaptedClient;
use opendev_tools_core::ToolRegistry;

/// Well-known subagent types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubagentType {
    CodeExplorer,
    Planner,
    General,
    Build,
    AskUser,
    Custom,
}

impl SubagentType {
    /// Parse a subagent type from a name string.
    pub fn from_name(name: &str) -> Self {
        match name {
            "Code-Explorer" | "code_explorer" => Self::CodeExplorer,
            "Planner" | "planner" => Self::Planner,
            "General" | "general" => Self::General,
            "Build" | "build" => Self::Build,
            "ask-user" | "ask_user" => Self::AskUser,
            _ => Self::Custom,
        }
    }

    /// Get the canonical name for this type.
    pub fn canonical_name(&self) -> &'static str {
        match self {
            Self::CodeExplorer => "Code-Explorer",
            Self::Planner => "Planner",
            Self::General => "General",
            Self::Build => "Build",
            Self::AskUser => "ask-user",
            Self::Custom => "custom",
        }
    }
}

/// Progress callback for subagent lifecycle events.
///
/// The parent TUI or caller can implement this to receive real-time
/// updates about the subagent's execution progress.
pub trait SubagentProgressCallback: Send + Sync {
    /// Called when the subagent starts executing.
    fn on_started(&self, subagent_name: &str, task: &str);

    /// Called when the subagent invokes a tool.
    fn on_tool_call(
        &self,
        subagent_name: &str,
        tool_name: &str,
        tool_id: &str,
        args: &HashMap<String, serde_json::Value>,
    );

    /// Called when a subagent tool call completes.
    fn on_tool_complete(&self, subagent_name: &str, tool_name: &str, tool_id: &str, success: bool);

    /// Called when the subagent finishes (with or without error).
    fn on_finished(&self, subagent_name: &str, success: bool, result_summary: &str);

    /// Called when token usage is reported from an LLM call.
    fn on_token_usage(&self, _subagent_name: &str, _input_tokens: u64, _output_tokens: u64) {}
}

/// A no-op progress callback for when the caller doesn't need progress updates.
#[derive(Debug)]
pub struct NoopProgressCallback;

impl SubagentProgressCallback for NoopProgressCallback {
    fn on_started(&self, _name: &str, _task: &str) {}
    fn on_tool_call(
        &self,
        _name: &str,
        _tool: &str,
        _id: &str,
        _args: &HashMap<String, serde_json::Value>,
    ) {
    }
    fn on_tool_complete(&self, _name: &str, _tool: &str, _id: &str, _success: bool) {}
    fn on_finished(&self, _name: &str, _success: bool, _summary: &str) {}
}

/// Bridge that forwards `AgentEventCallback` events from the subagent's
/// react loop to the parent's `SubagentProgressCallback`.
///
/// This makes subagent tool calls visible to the TUI in real-time.
pub struct SubagentEventBridge {
    subagent_name: String,
    progress: Arc<dyn SubagentProgressCallback>,
}

impl SubagentEventBridge {
    /// Create a new bridge for a given subagent.
    pub fn new(subagent_name: String, progress: Arc<dyn SubagentProgressCallback>) -> Self {
        Self {
            subagent_name,
            progress,
        }
    }
}

impl crate::traits::AgentEventCallback for SubagentEventBridge {
    fn on_tool_started(
        &self,
        tool_id: &str,
        tool_name: &str,
        args: &std::collections::HashMap<String, serde_json::Value>,
    ) {
        debug!(
            subagent = %self.subagent_name,
            tool_name = %tool_name,
            tool_id = %tool_id,
            "SubagentEventBridge: forwarding tool_started → on_tool_call"
        );
        self.progress
            .on_tool_call(&self.subagent_name, tool_name, tool_id, args);
    }

    fn on_tool_finished(&self, tool_id: &str, success: bool) {
        debug!(
            subagent = %self.subagent_name,
            tool_id = %tool_id,
            success = %success,
            "SubagentEventBridge: forwarding tool_finished → on_tool_complete"
        );
        self.progress
            .on_tool_complete(&self.subagent_name, "", tool_id, success);
    }

    fn on_agent_chunk(&self, _text: &str) {}
    fn on_thinking(&self, _content: &str) {}
    fn on_critique(&self, _content: &str) {}
    fn on_thinking_refined(&self, _content: &str) {}

    fn on_token_usage(&self, input_tokens: u64, output_tokens: u64) {
        self.progress
            .on_token_usage(&self.subagent_name, input_tokens, output_tokens);
    }
}

/// Select the appropriate runner for a subagent based on its type.
fn select_runner(spec: &SubAgentSpec, task: &str) -> Box<dyn super::runner::SubagentRunner> {
    use super::runner::{SimpleReactRunner, StandardReactRunner};

    match SubagentType::from_name(&spec.name) {
        SubagentType::CodeExplorer => {
            let max_iterations = spec.max_steps.unwrap_or(200) as usize;
            Box::new(SimpleReactRunner::new(max_iterations))
        }
        _ => {
            // Default to 25 iterations for non-Explorer agents (matches old behavior)
            let max_iterations = Some(spec.max_steps.unwrap_or(25) as usize);
            Box::new(StandardReactRunner::new(ReactLoopConfig {
                max_iterations,
                max_nudge_attempts: 3,
                max_todo_nudges: 2,
                thinking_level: opendev_runtime::ThinkingLevel::Off,
                original_task: Some(task.to_string()),
                permission: spec.permission.clone(),
                ..Default::default()
            }))
        }
    }
}

/// Result of spawning a subagent, containing the result and diagnostic info.
#[derive(Debug, Clone)]
pub struct SubagentRunResult {
    /// The agent result from the subagent's ReAct loop.
    pub agent_result: AgentResult,
    /// Number of tool calls the subagent made.
    pub tool_call_count: usize,
    /// Whether the shallow subagent warning applies.
    pub shallow_warning: Option<String>,
}

/// Manages subagent registration, lookup, and execution.
#[derive(Debug, Default)]
pub struct SubagentManager {
    specs: HashMap<String, SubAgentSpec>,
}

impl SubagentManager {
    /// Create a new empty manager.
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
        }
    }

    /// Create a manager pre-loaded with core built-in subagent specs.
    ///
    /// Registers only the essential subagents (Code-Explorer, Planner, project_init).
    /// Additional subagents can be loaded as custom agents from `~/.opendev/agents/*.md`.
    pub fn with_builtins() -> Self {
        use super::spec::builtins;
        use crate::prompts::embedded;

        let mut mgr = Self::new();
        mgr.register(builtins::code_explorer(
            embedded::SUBAGENTS_SUBAGENT_CODE_EXPLORER,
        ));
        mgr.register(builtins::planner(embedded::SUBAGENTS_SUBAGENT_PLANNER));
        mgr.register(builtins::general(
            "You are a versatile coding assistant. Complete the task using all tools available to you. \
             Read files, search code, edit files, run commands, and use web tools as needed. \
             Be thorough and methodical.",
        ));
        mgr.register(builtins::build(
            "You are a build and test runner. Your job is to run builds, analyze errors, \
             fix compilation failures, and ensure tests pass. Focus on the build output \
             and fix issues systematically.",
        ));
        mgr.register(builtins::project_init(
            embedded::SUBAGENTS_SUBAGENT_PROJECT_INIT,
        ));
        mgr
    }

    /// Create a manager with built-in specs plus custom agents loaded from disk.
    ///
    /// Scans agent directories from lowest to highest priority:
    /// 1. Global: `~/.opendev/agents/`, `~/.agents/agents/`, `~/.claude/agents/`
    /// 2. Walk from git root down to working_dir: `.opendev/agents/`, `.agents/agents/`,
    ///    `.claude/agents/` at each level (monorepo support)
    ///
    /// Custom agents override built-ins with the same name.
    pub fn with_builtins_and_custom(working_dir: &std::path::Path) -> Self {
        let mut mgr = Self::with_builtins();
        let home = dirs::home_dir().unwrap_or_default();

        // Determine git root for monorepo walking
        let git_root = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(working_dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| std::path::PathBuf::from(s.trim()))
            });
        let stop_dir = git_root.as_deref().unwrap_or(working_dir);

        // Order: lowest priority first (later register() calls override earlier).
        let mut dirs = Vec::new();

        // 1. Global dirs (lowest priority)
        for subdir in &[".opendev", ".agents", ".claude"] {
            dirs.push(home.join(subdir).join("agents"));
        }

        // 2. Walk from working_dir up to git root, collect directory levels
        //    Parent dirs have lower priority than child dirs.
        //    Within each level: .opendev < .agents < .claude
        let mut levels: Vec<std::path::PathBuf> = Vec::new();
        {
            let mut current = working_dir.to_path_buf();
            loop {
                levels.push(current.clone());
                if current == stop_dir || !current.pop() {
                    break;
                }
            }
        }
        // Reverse so parent dirs (lower priority) load first, working_dir loads last
        levels.reverse();
        for level in &levels {
            for subdir in &[".opendev", ".agents", ".claude"] {
                dirs.push(level.join(subdir).join("agents"));
            }
        }

        for spec in super::custom_loader::load_custom_agents(&dirs) {
            mgr.register(spec);
        }
        mgr
    }

    /// Apply inline agent config overrides from `opendev.json`.
    ///
    /// For each entry in the config map:
    /// - If `disable: true`, removes the agent entirely
    /// - If the agent exists, merges the overrides onto it
    /// - If the agent doesn't exist, creates a new custom agent
    pub fn apply_config_overrides(
        &mut self,
        overrides: &std::collections::HashMap<String, opendev_models::AgentConfigInline>,
    ) {
        use super::spec::PermissionAction;

        for (name, cfg) in overrides {
            // Handle disable
            if cfg.disable == Some(true) {
                if self.specs.remove(name).is_some() {
                    info!(agent = name, "Disabled agent via config override");
                }
                continue;
            }

            let spec = self.specs.entry(name.clone()).or_insert_with(|| {
                info!(agent = name, "Creating new agent from config");
                SubAgentSpec::new(
                    name,
                    cfg.description.as_deref().unwrap_or("Custom agent"),
                    cfg.prompt
                        .as_deref()
                        .unwrap_or("You are a helpful assistant."),
                )
            });

            // Apply overrides
            if let Some(ref model) = cfg.model {
                spec.model = Some(model.clone());
            }
            if let Some(ref prompt) = cfg.prompt {
                spec.system_prompt = prompt.clone();
            }
            if let Some(ref desc) = cfg.description {
                spec.description = desc.clone();
            }
            if let Some(temp) = cfg.temperature {
                spec.temperature = Some(temp as f32);
            }
            if let Some(top_p) = cfg.top_p {
                spec.top_p = Some(top_p as f32);
            }
            if let Some(steps) = cfg.max_steps {
                spec.max_steps = Some(steps as u32);
            }
            if let Some(ref color) = cfg.color {
                spec.color = Some(color.clone());
            }
            if let Some(hidden) = cfg.hidden {
                spec.hidden = hidden;
            }
            if let Some(ref mode) = cfg.mode {
                spec.mode = super::spec::AgentMode::parse_mode(mode);
            }

            // Merge permissions (config overrides existing)
            for (tool_pattern, action_str) in &cfg.permission {
                let action = match action_str.as_str() {
                    "allow" => PermissionAction::Allow,
                    "deny" => PermissionAction::Deny,
                    "ask" => PermissionAction::Ask,
                    _ => continue,
                };
                spec.permission.insert(
                    tool_pattern.clone(),
                    super::spec::PermissionRule::Action(action),
                );
            }
        }
    }

    /// Register a subagent specification.
    pub fn register(&mut self, spec: SubAgentSpec) {
        self.specs.insert(spec.name.clone(), spec);
    }

    /// Get a subagent spec by name.
    pub fn get(&self, name: &str) -> Option<&SubAgentSpec> {
        self.specs.get(name)
    }

    /// Get a subagent spec by type.
    pub fn get_by_type(&self, subagent_type: SubagentType) -> Option<&SubAgentSpec> {
        self.specs.get(subagent_type.canonical_name())
    }

    /// List all registered subagent names (excludes hidden and disabled agents).
    pub fn names(&self) -> Vec<&str> {
        self.specs
            .values()
            .filter(|s| !s.hidden && !s.disable)
            .map(|s| s.name.as_str())
            .collect()
    }

    /// List all registered subagent names including hidden ones.
    pub fn all_names(&self) -> Vec<&str> {
        self.specs.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of registered subagents.
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// Check if the manager is empty.
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Unregister a subagent by name.
    pub fn unregister(&mut self, name: &str) -> Option<SubAgentSpec> {
        self.specs.remove(name)
    }

    /// Resolve the default agent name for new sessions.
    ///
    /// If `configured_default` is `Some`, validates that the agent exists,
    /// is not hidden, and can be used as a primary agent. Falls back to
    /// the first non-hidden primary-capable agent, or `None` if no suitable
    /// agent is found.
    pub fn resolve_default_agent(&self, configured_default: Option<&str>) -> Option<&str> {
        if let Some(name) = configured_default {
            if let Some(spec) = self.specs.get(name) {
                if spec.disable {
                    tracing::warn!(agent = name, "default_agent is disabled, falling back");
                } else if spec.hidden {
                    tracing::warn!(agent = name, "default_agent is hidden, falling back");
                } else if !spec.mode.can_be_primary() {
                    tracing::warn!(agent = name, "default_agent is subagent-only, falling back");
                } else {
                    return Some(&spec.name);
                }
            } else {
                tracing::warn!(agent = name, "default_agent not found, falling back");
            }
        }

        // Fallback: first non-hidden, non-disabled, primary-capable agent
        self.specs
            .values()
            .find(|s| !s.hidden && !s.disable && s.mode.can_be_primary())
            .map(|s| s.name.as_str())
    }

    /// Build tool schemas description listing available subagents.
    ///
    /// Used to populate the `subagent_type` enum in the `spawn_subagent` tool schema.
    /// Excludes hidden and disabled agents.
    pub fn build_enum_description(&self) -> Vec<(String, String)> {
        self.specs
            .values()
            .filter(|s| !s.hidden && !s.disable)
            .map(|s| (s.name.clone(), s.description.clone()))
            .collect()
    }

    /// Spawn and run a subagent with the given task.
    ///
    /// Creates an isolated `MainAgent` with the subagent's restricted tool set,
    /// system prompt, and optional model override. Runs the subagent's own ReAct
    /// loop and returns the result along with diagnostic information.
    ///
    /// # Arguments
    /// * `subagent_name` - Name of the registered subagent spec
    /// * `task` - The task description to send to the subagent
    /// * `parent_model` - Model to use if the spec doesn't override
    /// * `tool_registry` - Full tool registry (subagent tools will be filtered)
    /// * `http_client` - HTTP client for LLM API calls
    /// * `working_dir` - Working directory for tool execution
    /// * `progress` - Callback for progress updates
    /// * `task_monitor` - Optional interrupt monitor
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn(
        &self,
        subagent_name: &str,
        task: &str,
        parent_model: &str,
        tool_registry: Arc<ToolRegistry>,
        http_client: Arc<AdaptedClient>,
        working_dir: &str,
        progress: Arc<dyn SubagentProgressCallback>,
        _task_monitor: Option<&dyn TaskMonitor>,
        tool_approval_tx: Option<&opendev_runtime::ToolApprovalSender>,
        parent_max_tokens: u64,
    ) -> Result<SubagentRunResult, AgentError> {
        let spec = self.get(subagent_name).ok_or_else(|| {
            AgentError::ConfigError(format!("Unknown subagent type: {subagent_name}"))
        })?;

        // Block spawning disabled agents.
        if spec.disable {
            return Err(AgentError::ConfigError(format!(
                "Agent '{subagent_name}' is disabled"
            )));
        }

        info!(
            subagent = %spec.name,
            task_len = task.len(),
            tool_count = spec.tools.len(),
            "Spawning subagent"
        );

        progress.on_started(&spec.name, task);

        // Determine model (spec override or parent's model)
        let model = spec.model.as_deref().unwrap_or(parent_model).to_string();

        // Build restricted tool list (if specified)
        let mut allowed_tools = if spec.has_tool_restriction() {
            Some(spec.tools.clone())
        } else {
            None
        };

        // Remove tools that have blanket deny in permission rules.
        // These are completely hidden from the LLM so it won't even attempt them.
        if !spec.permission.is_empty() {
            let all_names = tool_registry.tool_names();
            let all_refs: Vec<&str> = all_names.iter().map(|s| s.as_str()).collect();
            let denied = spec.disabled_tools(&all_refs);
            if !denied.is_empty() {
                let tools = allowed_tools.get_or_insert_with(|| all_names.clone());
                tools.retain(|t| !denied.contains(t));
                debug!(
                    subagent = %spec.name,
                    denied_tools = ?denied,
                    "Removed permission-denied tools from schema"
                );
            }
        }

        // Build the subagent's system prompt by combining the spec prompt
        // with project instruction files (AGENTS.md, CLAUDE.md, etc.) so
        // subagents follow the same project rules as the main agent.
        let system_prompt = {
            let wd = std::path::Path::new(working_dir);
            let instructions = opendev_context::discover_instruction_files(wd);
            if instructions.is_empty() {
                spec.system_prompt.clone()
            } else {
                let mut parts = vec![spec.system_prompt.clone()];
                parts.push("\n\n# Project Instructions\n".to_string());
                for instr in &instructions {
                    let filename = instr.path.file_name().unwrap_or_default().to_string_lossy();
                    parts.push(format!(
                        "## {} ({})\n{}",
                        filename, instr.scope, instr.content
                    ));
                }
                parts.join("\n")
            }
        };

        // Build LlmCaller with subagent config
        let temperature = spec.temperature.map(|t| t as f64).unwrap_or(0.7);
        let llm_caller = crate::llm_calls::LlmCaller::new(crate::llm_calls::LlmCallConfig {
            model: model.clone(),
            temperature: Some(temperature),
            max_tokens: Some(spec.max_tokens.unwrap_or(parent_max_tokens as u32) as u64),
        });

        // Build tool schemas (filtered to allowed tools)
        let tool_schemas = crate::main_agent::MainAgent::build_schemas_pub(
            &tool_registry,
            allowed_tools.as_deref(),
        );

        // Build tool context
        let mut tool_context = opendev_tools_core::ToolContext::new(working_dir);
        tool_context.is_subagent = true;

        // Wire event bridge so subagent tool calls are visible to the TUI
        let bridge = Arc::new(SubagentEventBridge::new(
            spec.name.clone(),
            Arc::clone(&progress),
        ));

        // Select the runner based on subagent type
        let runner = select_runner(spec, task);

        debug!(
            subagent = %spec.name,
            runner = runner.name(),
            "Running subagent via runner"
        );

        // Prepare initial messages
        let mut messages = vec![serde_json::json!({"role": "system", "content": system_prompt})];

        // Auto-scout: inject project structure for Code-Explorer so it doesn't
        // waste tool calls discovering layout (and parallel explorers see the
        // same tree, helping them pick different areas).
        if matches!(
            SubagentType::from_name(&spec.name),
            SubagentType::CodeExplorer
        ) {
            let structure = scan_project_structure(std::path::Path::new(working_dir), 3);
            if !structure.is_empty() {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": format!(
                        "Here is the project structure to help you navigate:\n\n{structure}\n\
                        IMPORTANT: Only use paths that appear in this tree or that you discover via list_files. \
                        Do NOT guess or hallucinate paths — if you're unsure whether a directory or file exists, \
                        use list_files to check first."
                    )
                }));
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": "Thank you for the project structure. I'll only use paths from this tree or ones I discover via list_files — no guessing. Let me now address your task."
                }));
            } else {
                warn!(
                    subagent = %spec.name,
                    working_dir = %working_dir,
                    "Auto-scout: project structure scan returned empty — working directory may be invalid"
                );
            }
        }

        messages.push(serde_json::json!({"role": "user", "content": task}));

        // Build runner context
        let runner_ctx = super::runner::RunnerContext {
            caller: &llm_caller,
            http_client: &http_client,
            tool_schemas: &tool_schemas,
            tool_registry: &tool_registry,
            tool_context: &tool_context,
            event_callback: Some(bridge.as_ref() as &dyn crate::traits::AgentEventCallback),
            cancel: None, // Subagents don't support cancellation tokens yet
            tool_approval_tx: tool_approval_tx,
        };

        // Run the isolated ReAct loop via the selected runner
        let result = runner.run(&runner_ctx, &mut messages).await;

        match result {
            Ok(agent_result) => {
                // Count tool calls for shallow subagent detection
                let tool_call_count = ReactLoop::count_subagent_tool_calls(&agent_result.messages);
                let shallow_warning = ReactLoop::shallow_subagent_warning(
                    &agent_result.messages,
                    agent_result.success,
                );

                if let Some(ref warning) = shallow_warning {
                    warn!(
                        subagent = %spec.name,
                        tool_calls = tool_call_count,
                        "Shallow subagent detected"
                    );
                    debug!("{}", warning);
                }

                let summary = if agent_result.content.len() > 200 {
                    format!("{}...", &agent_result.content[..200])
                } else {
                    agent_result.content.clone()
                };
                progress.on_finished(&spec.name, agent_result.success, &summary);

                Ok(SubagentRunResult {
                    agent_result,
                    tool_call_count,
                    shallow_warning,
                })
            }
            Err(e) => {
                let err_msg = e.to_string();
                progress.on_finished(&spec.name, false, &err_msg);
                Err(e)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Auto-scout: project structure scanner for Code-Explorer
// ---------------------------------------------------------------------------

/// Directories to skip when scanning project structure.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "__pycache__",
    "dist",
    "build",
    "vendor",
    ".venv",
    ".vscode",
    ".idea",
    ".next",
    ".cache",
    ".tox",
    "venv",
    "env",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
];

/// Cap on total entries to avoid flooding context.
const MAX_SCAN_ENTRIES: usize = 100;

/// Scan a directory tree up to `max_depth` levels and return a tree-style
/// string listing. Returns an empty string if the directory cannot be read.
fn scan_project_structure(root: &std::path::Path, max_depth: usize) -> String {
    let mut entries: Vec<String> = Vec::new();
    scan_dir(root, "", max_depth, 0, &mut entries);
    if entries.is_empty() {
        return String::new();
    }
    let mut out = String::from("Project structure:\n");
    for line in &entries {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Recursive helper that collects tree lines into `out`.
fn scan_dir(
    dir: &std::path::Path,
    prefix: &str,
    max_depth: usize,
    depth: usize,
    out: &mut Vec<String>,
) {
    if out.len() >= MAX_SCAN_ENTRIES {
        return;
    }

    let mut children: Vec<std::fs::DirEntry> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };

    // Sort entries: directories first, then alphabetical.
    children.sort_by(|a, b| {
        let a_dir = a.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        b_dir
            .cmp(&a_dir)
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });

    let total = children.len();
    for (i, entry) in children.into_iter().enumerate() {
        if out.len() >= MAX_SCAN_ENTRIES {
            out.push(format!("{prefix}... (truncated)"));
            return;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let is_last = i == total - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

        if is_dir {
            // Skip excluded directories
            if SKIP_DIRS.iter().any(|&s| s == name_str) {
                continue;
            }
            out.push(format!("{prefix}{connector}{name_str}/"));
            if depth < max_depth {
                let child_prefix = if is_last {
                    format!("{prefix}    ")
                } else {
                    format!("{prefix}│   ")
                };
                scan_dir(&entry.path(), &child_prefix, max_depth, depth + 1, out);
            }
        } else {
            out.push(format!("{prefix}{connector}{name_str}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_spec(name: &str) -> SubAgentSpec {
        SubAgentSpec::new(name, format!("Description of {name}"), "system prompt")
    }

    #[test]
    fn test_manager_new_empty() {
        let mgr = SubagentManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_register_and_get() {
        let mut mgr = SubagentManager::new();
        mgr.register(make_spec("Code-Explorer"));
        assert_eq!(mgr.len(), 1);
        assert!(mgr.get("Code-Explorer").is_some());
        assert!(mgr.get("nonexistent").is_none());
    }

    #[test]
    fn test_get_by_type() {
        let mut mgr = SubagentManager::new();
        mgr.register(make_spec("Code-Explorer"));
        assert!(mgr.get_by_type(SubagentType::CodeExplorer).is_some());
        assert!(mgr.get_by_type(SubagentType::Planner).is_none());
    }

    #[test]
    fn test_unregister() {
        let mut mgr = SubagentManager::new();
        mgr.register(make_spec("Planner"));
        assert!(mgr.unregister("Planner").is_some());
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_names() {
        let mut mgr = SubagentManager::new();
        mgr.register(make_spec("A"));
        mgr.register(make_spec("B"));
        let names = mgr.names();
        assert!(names.contains(&"A"));
        assert!(names.contains(&"B"));
    }

    #[test]
    fn test_build_enum_description() {
        let mut mgr = SubagentManager::new();
        mgr.register(make_spec("Code-Explorer"));
        let descs = mgr.build_enum_description();
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].0, "Code-Explorer");
    }

    #[test]
    fn test_subagent_type_from_name() {
        assert_eq!(
            SubagentType::from_name("Code-Explorer"),
            SubagentType::CodeExplorer
        );
        assert_eq!(SubagentType::from_name("Planner"), SubagentType::Planner);
        assert_eq!(SubagentType::from_name("General"), SubagentType::General);
        assert_eq!(SubagentType::from_name("general"), SubagentType::General);
        assert_eq!(SubagentType::from_name("Build"), SubagentType::Build);
        assert_eq!(SubagentType::from_name("build"), SubagentType::Build);
        assert_eq!(SubagentType::from_name("ask-user"), SubagentType::AskUser);
        assert_eq!(SubagentType::from_name("unknown"), SubagentType::Custom);
    }

    #[test]
    fn test_subagent_type_canonical_name() {
        assert_eq!(SubagentType::CodeExplorer.canonical_name(), "Code-Explorer");
        assert_eq!(SubagentType::General.canonical_name(), "General");
        assert_eq!(SubagentType::Build.canonical_name(), "Build");
        assert_eq!(SubagentType::AskUser.canonical_name(), "ask-user");
    }

    #[test]
    fn test_with_builtins() {
        let mgr = SubagentManager::with_builtins();
        assert_eq!(mgr.len(), 5);
        assert!(mgr.get("Code-Explorer").is_some());
        assert!(mgr.get("Planner").is_some());
        assert!(mgr.get("General").is_some());
        assert!(mgr.get("Build").is_some());
        assert!(mgr.get("project_init").is_some());
    }

    #[test]
    fn test_with_builtins_and_custom() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join(".opendev").join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("test-agent.md"),
            "---\ndescription: Test agent\n---\nYou are a test.",
        )
        .unwrap();

        let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
        assert!(mgr.len() >= 4); // 3 builtins + 1 custom
        assert!(mgr.get("test-agent").is_some());
    }

    #[test]
    fn test_agents_from_dot_agents_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join(".agents").join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("reviewer.md"),
            "---\ndescription: Code reviewer from .agents\n---\nYou review code.",
        )
        .unwrap();

        let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
        assert!(mgr.get("reviewer").is_some());
        let spec = mgr.get("reviewer").unwrap();
        assert!(spec.system_prompt.contains("review code"));
    }

    #[test]
    fn test_agent_priority_claude_over_agents_over_opendev() {
        let tmp = tempfile::tempdir().unwrap();

        // Create same-named agent in all three dirs
        let opendev_dir = tmp.path().join(".opendev").join("agents");
        let agents_dir = tmp.path().join(".agents").join("agents");
        let claude_dir = tmp.path().join(".claude").join("agents");
        std::fs::create_dir_all(&opendev_dir).unwrap();
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::create_dir_all(&claude_dir).unwrap();

        std::fs::write(
            opendev_dir.join("shared.md"),
            "---\ndescription: From .opendev\n---\nOpenDev version.",
        )
        .unwrap();
        std::fs::write(
            agents_dir.join("shared.md"),
            "---\ndescription: From .agents\n---\nAgents version.",
        )
        .unwrap();
        std::fs::write(
            claude_dir.join("shared.md"),
            "---\ndescription: From .claude\n---\nClaude version.",
        )
        .unwrap();

        let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
        let spec = mgr.get("shared").unwrap();
        // .claude has highest priority
        assert_eq!(spec.description, "From .claude");
    }

    #[test]
    fn test_hidden_agents_excluded_from_names() {
        let mut mgr = SubagentManager::new();
        mgr.register(make_spec("visible"));
        mgr.register(SubAgentSpec::new("hidden-agent", "Hidden", "prompt").with_hidden(true));

        let names = mgr.names();
        assert!(names.contains(&"visible"));
        assert!(!names.contains(&"hidden-agent"));

        // all_names includes hidden
        let all = mgr.all_names();
        assert!(all.contains(&"visible"));
        assert!(all.contains(&"hidden-agent"));
    }

    #[test]
    fn test_hidden_agents_excluded_from_enum_description() {
        let mut mgr = SubagentManager::new();
        mgr.register(make_spec("visible"));
        mgr.register(SubAgentSpec::new("hidden-agent", "Hidden", "prompt").with_hidden(true));

        let descs = mgr.build_enum_description();
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].0, "visible");
    }

    #[test]
    fn test_hidden_agents_still_gettable() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("hidden-agent", "Hidden", "prompt").with_hidden(true));

        // Hidden agents can still be retrieved by name (for programmatic spawning)
        assert!(mgr.get("hidden-agent").is_some());
    }

    #[test]
    fn test_custom_agent_with_steps_and_temperature() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join(".opendev").join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("custom.md"),
            "---\ndescription: Custom\nsteps: 50\ntemperature: 0.3\nhidden: true\n---\nYou are custom.",
        )
        .unwrap();

        let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
        let spec = mgr.get("custom").unwrap();
        assert_eq!(spec.max_steps, Some(50));
        assert_eq!(spec.temperature, Some(0.3));
        assert!(spec.hidden);

        // Hidden agent excluded from names
        assert!(!mgr.names().contains(&"custom"));
    }

    #[test]
    fn test_custom_agent_overrides_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join(".opendev").join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();
        // Create a custom agent with same name as a builtin
        std::fs::write(
            agent_dir.join("Code-Explorer.md"),
            "---\ndescription: Custom explorer\ntemperature: 0.1\n---\nCustom explorer prompt.",
        )
        .unwrap();

        let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
        let spec = mgr.get("Code-Explorer").unwrap();
        // Custom should override the builtin
        assert!(spec.system_prompt.contains("Custom explorer prompt"));
        assert_eq!(spec.temperature, Some(0.1));
    }

    #[test]
    fn test_with_claude_agents_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let claude_dir = tmp.path().join(".claude").join("agents");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("claude-agent.md"),
            "---\ndescription: Claude agent\n---\nClaude agent prompt.",
        )
        .unwrap();

        let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
        assert!(mgr.get("claude-agent").is_some());
    }

    #[test]
    fn test_claude_agents_higher_priority_than_opendev() {
        let tmp = tempfile::tempdir().unwrap();

        // Create same-named agent in both directories
        let claude_dir = tmp.path().join(".claude").join("agents");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("reviewer.md"),
            "---\ndescription: Claude reviewer\n---\nClaude reviewer.",
        )
        .unwrap();

        let opendev_dir = tmp.path().join(".opendev").join("agents");
        std::fs::create_dir_all(&opendev_dir).unwrap();
        std::fs::write(
            opendev_dir.join("reviewer.md"),
            "---\ndescription: OpenDev reviewer\n---\nOpenDev reviewer.",
        )
        .unwrap();

        let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
        let spec = mgr.get("reviewer").unwrap();
        // .claude/ is loaded last (highest priority), so it wins
        assert!(
            spec.system_prompt.contains("Claude reviewer"),
            "Claude agent should override OpenDev agent, got: {}",
            spec.system_prompt
        );
    }

    // ---- Disabled agent filtering ----

    #[test]
    fn test_disabled_agents_excluded_from_names() {
        let mut mgr = SubagentManager::new();
        mgr.register(make_spec("active"));
        mgr.register(SubAgentSpec::new("disabled-agent", "Disabled", "prompt").with_disable(true));

        let names = mgr.names();
        assert!(names.contains(&"active"));
        assert!(
            !names.contains(&"disabled-agent"),
            "Disabled agents should be excluded from names()"
        );
    }

    #[test]
    fn test_disabled_agents_excluded_from_enum_description() {
        let mut mgr = SubagentManager::new();
        mgr.register(make_spec("active"));
        mgr.register(SubAgentSpec::new("disabled-agent", "Disabled", "prompt").with_disable(true));

        let descs = mgr.build_enum_description();
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].0, "active");
    }

    #[test]
    fn test_disabled_agents_still_gettable() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("disabled-agent", "Disabled", "prompt").with_disable(true));

        // Disabled agents can still be looked up (but spawn will fail)
        assert!(mgr.get("disabled-agent").is_some());
    }

    #[test]
    fn test_disabled_agents_in_all_names() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("disabled-agent", "Disabled", "prompt").with_disable(true));

        let all = mgr.all_names();
        assert!(
            all.contains(&"disabled-agent"),
            "all_names() should include disabled agents"
        );
    }

    // ---- apply_config_overrides tests ----

    #[test]
    fn test_config_override_model() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("build", "Build agent", "Build things."));

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "build".to_string(),
            opendev_models::AgentConfigInline {
                model: Some("gpt-4o-mini".to_string()),
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        let spec = mgr.get("build").unwrap();
        assert_eq!(spec.model.as_deref(), Some("gpt-4o-mini"));
    }

    #[test]
    fn test_config_override_temperature_and_top_p() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("test", "Test", "prompt"));

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "test".to_string(),
            opendev_models::AgentConfigInline {
                temperature: Some(0.7),
                top_p: Some(0.9),
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        let spec = mgr.get("test").unwrap();
        assert!((spec.temperature.unwrap() - 0.7).abs() < 0.01);
        assert!((spec.top_p.unwrap() - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_config_override_disable_removes_agent() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("build", "Build agent", "prompt"));
        assert!(mgr.get("build").is_some());

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "build".to_string(),
            opendev_models::AgentConfigInline {
                disable: Some(true),
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        assert!(
            mgr.get("build").is_none(),
            "Disabled agent should be removed"
        );
    }

    #[test]
    fn test_config_override_creates_new_agent() {
        let mut mgr = SubagentManager::new();
        assert!(mgr.get("custom-agent").is_none());

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "custom-agent".to_string(),
            opendev_models::AgentConfigInline {
                description: Some("My custom agent".to_string()),
                prompt: Some("Be creative.".to_string()),
                model: Some("claude-opus-4-5".to_string()),
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        let spec = mgr.get("custom-agent").unwrap();
        assert_eq!(spec.description, "My custom agent");
        assert_eq!(spec.system_prompt, "Be creative.");
        assert_eq!(spec.model.as_deref(), Some("claude-opus-4-5"));
    }

    #[test]
    fn test_config_override_prompt_and_description() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("build", "Old desc", "Old prompt"));

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "build".to_string(),
            opendev_models::AgentConfigInline {
                description: Some("New description".to_string()),
                prompt: Some("New system prompt".to_string()),
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        let spec = mgr.get("build").unwrap();
        assert_eq!(spec.description, "New description");
        assert_eq!(spec.system_prompt, "New system prompt");
    }

    #[test]
    fn test_config_override_max_steps_and_color() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("build", "Build", "prompt"));

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "build".to_string(),
            opendev_models::AgentConfigInline {
                max_steps: Some(50),
                color: Some("#FF6600".to_string()),
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        let spec = mgr.get("build").unwrap();
        assert_eq!(spec.max_steps, Some(50));
        assert_eq!(spec.color.as_deref(), Some("#FF6600"));
    }

    #[test]
    fn test_config_override_hidden() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("build", "Build", "prompt"));

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "build".to_string(),
            opendev_models::AgentConfigInline {
                hidden: Some(true),
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        let spec = mgr.get("build").unwrap();
        assert!(spec.hidden);
        assert!(!mgr.names().contains(&"build"));
    }

    #[test]
    fn test_config_override_mode() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("build", "Build", "prompt"));

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "build".to_string(),
            opendev_models::AgentConfigInline {
                mode: Some("primary".to_string()),
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        let spec = mgr.get("build").unwrap();
        assert!(spec.mode.can_be_primary());
    }

    #[test]
    fn test_config_override_permission_rules() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("build", "Build", "prompt"));

        let mut perms = std::collections::HashMap::new();
        perms.insert("bash".to_string(), "deny".to_string());
        perms.insert("edit".to_string(), "allow".to_string());

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "build".to_string(),
            opendev_models::AgentConfigInline {
                permission: perms,
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        let spec = mgr.get("build").unwrap();
        // Check bash is denied
        let bash_action = spec.evaluate_permission("bash", "");
        assert_eq!(bash_action, Some(crate::subagents::PermissionAction::Deny));
        // Check edit is allowed
        let edit_action = spec.evaluate_permission("edit", "");
        assert_eq!(edit_action, Some(crate::subagents::PermissionAction::Allow));
    }

    #[test]
    fn test_config_override_invalid_permission_action_skipped() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("build", "Build", "prompt"));

        let mut perms = std::collections::HashMap::new();
        perms.insert("bash".to_string(), "invalid_action".to_string());

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "build".to_string(),
            opendev_models::AgentConfigInline {
                permission: perms,
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        let spec = mgr.get("build").unwrap();
        // Invalid action should be skipped, so no permission rule for bash
        assert_eq!(spec.evaluate_permission("bash", ""), None);
    }

    #[test]
    fn test_config_override_multiple_agents() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("build", "Build", "prompt1"));
        mgr.register(SubAgentSpec::new("explore", "Explore", "prompt2"));

        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "build".to_string(),
            opendev_models::AgentConfigInline {
                model: Some("gpt-4o".to_string()),
                ..Default::default()
            },
        );
        overrides.insert(
            "explore".to_string(),
            opendev_models::AgentConfigInline {
                temperature: Some(0.2),
                ..Default::default()
            },
        );
        mgr.apply_config_overrides(&overrides);

        assert_eq!(mgr.get("build").unwrap().model.as_deref(), Some("gpt-4o"));
        assert!((mgr.get("explore").unwrap().temperature.unwrap() - 0.2).abs() < 0.01);
    }

    // ---- resolve_default_agent tests ----

    #[test]
    fn test_resolve_default_agent_configured() {
        let mut mgr = SubagentManager::new();
        mgr.register(
            SubAgentSpec::new("build", "Build", "prompt")
                .with_mode(crate::subagents::AgentMode::All),
        );

        let result = mgr.resolve_default_agent(Some("build"));
        assert_eq!(result, Some("build"));
    }

    #[test]
    fn test_resolve_default_agent_not_found_falls_back() {
        let mut mgr = SubagentManager::new();
        mgr.register(
            SubAgentSpec::new("build", "Build", "prompt")
                .with_mode(crate::subagents::AgentMode::All),
        );

        let result = mgr.resolve_default_agent(Some("nonexistent"));
        assert_eq!(
            result,
            Some("build"),
            "Should fall back to first primary-capable agent"
        );
    }

    #[test]
    fn test_resolve_default_agent_disabled_falls_back() {
        let mut mgr = SubagentManager::new();
        mgr.register(
            SubAgentSpec::new("build", "Build", "prompt")
                .with_mode(crate::subagents::AgentMode::All)
                .with_disable(true),
        );
        mgr.register(
            SubAgentSpec::new("general", "General", "prompt")
                .with_mode(crate::subagents::AgentMode::Primary),
        );

        let result = mgr.resolve_default_agent(Some("build"));
        assert_eq!(
            result,
            Some("general"),
            "Should skip disabled and fall back"
        );
    }

    #[test]
    fn test_resolve_default_agent_hidden_falls_back() {
        let mut mgr = SubagentManager::new();
        let mut hidden = SubAgentSpec::new("hidden-agent", "Hidden", "prompt");
        hidden.hidden = true;
        hidden.mode = crate::subagents::AgentMode::All;
        mgr.register(hidden);
        mgr.register(
            SubAgentSpec::new("visible", "Visible", "prompt")
                .with_mode(crate::subagents::AgentMode::Primary),
        );

        let result = mgr.resolve_default_agent(Some("hidden-agent"));
        assert_eq!(result, Some("visible"), "Should skip hidden and fall back");
    }

    #[test]
    fn test_resolve_default_agent_subagent_only_falls_back() {
        let mut mgr = SubagentManager::new();
        mgr.register(SubAgentSpec::new("helper", "Helper", "prompt"));
        // Default mode is Subagent, which can't be primary
        mgr.register(
            SubAgentSpec::new("primary", "Primary", "prompt")
                .with_mode(crate::subagents::AgentMode::Primary),
        );

        let result = mgr.resolve_default_agent(Some("helper"));
        assert_eq!(
            result,
            Some("primary"),
            "Should skip subagent-only and fall back"
        );
    }

    #[test]
    fn test_resolve_default_agent_none_configured() {
        let mut mgr = SubagentManager::new();
        mgr.register(
            SubAgentSpec::new("build", "Build", "prompt")
                .with_mode(crate::subagents::AgentMode::All),
        );

        let result = mgr.resolve_default_agent(None);
        assert_eq!(
            result,
            Some("build"),
            "Should return first primary-capable agent"
        );
    }

    #[test]
    fn test_resolve_default_agent_no_primary_capable() {
        let mut mgr = SubagentManager::new();
        // Only subagent-mode agents
        mgr.register(SubAgentSpec::new("helper1", "Helper 1", "prompt"));
        mgr.register(SubAgentSpec::new("helper2", "Helper 2", "prompt"));

        let result = mgr.resolve_default_agent(None);
        assert_eq!(result, None, "No primary-capable agents → None");
    }

    // --- SubagentEventBridge tests ---

    /// Mock progress callback that records events.
    struct RecordingProgressCallback {
        events: std::sync::Mutex<Vec<String>>,
    }

    impl RecordingProgressCallback {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(Vec::new()),
            }
        }
        fn events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
    }

    impl SubagentProgressCallback for RecordingProgressCallback {
        fn on_started(&self, name: &str, task: &str) {
            self.events
                .lock()
                .unwrap()
                .push(format!("started:{name}:{task}"));
        }
        fn on_tool_call(
            &self,
            name: &str,
            tool: &str,
            id: &str,
            _args: &HashMap<String, serde_json::Value>,
        ) {
            self.events
                .lock()
                .unwrap()
                .push(format!("tool_call:{name}:{tool}:{id}"));
        }
        fn on_tool_complete(&self, name: &str, _tool: &str, id: &str, success: bool) {
            self.events
                .lock()
                .unwrap()
                .push(format!("tool_complete:{name}:{id}:{success}"));
        }
        fn on_finished(&self, name: &str, success: bool, _summary: &str) {
            self.events
                .lock()
                .unwrap()
                .push(format!("finished:{name}:{success}"));
        }
        fn on_token_usage(&self, name: &str, input: u64, output: u64) {
            self.events
                .lock()
                .unwrap()
                .push(format!("tokens:{name}:{input}:{output}"));
        }
    }

    #[test]
    fn test_event_bridge_forwards_tool_started() {
        let recorder = Arc::new(RecordingProgressCallback::new());
        let progress: Arc<dyn SubagentProgressCallback> = Arc::clone(&recorder) as _;
        let bridge = SubagentEventBridge::new("test-agent".to_string(), progress);

        use crate::traits::AgentEventCallback;
        let args = std::collections::HashMap::new();
        bridge.on_tool_started("tc-1", "read_file", &args);

        let events = recorder.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "tool_call:test-agent:read_file:tc-1");
    }

    #[test]
    fn test_event_bridge_forwards_tool_finished() {
        let recorder = Arc::new(RecordingProgressCallback::new());
        let progress: Arc<dyn SubagentProgressCallback> = Arc::clone(&recorder) as _;
        let bridge = SubagentEventBridge::new("test-agent".to_string(), progress);

        use crate::traits::AgentEventCallback;
        bridge.on_tool_finished("tc-1", true);

        let events = recorder.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "tool_complete:test-agent:tc-1:true");
    }

    #[test]
    fn test_event_bridge_forwards_token_usage() {
        let recorder = Arc::new(RecordingProgressCallback::new());
        let progress: Arc<dyn SubagentProgressCallback> = Arc::clone(&recorder) as _;
        let bridge = SubagentEventBridge::new("test-agent".to_string(), progress);

        use crate::traits::AgentEventCallback;
        bridge.on_token_usage(1000, 500);

        let events = recorder.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "tokens:test-agent:1000:500");
    }

    #[test]
    fn test_event_bridge_noop_methods() {
        let recorder = Arc::new(RecordingProgressCallback::new());
        let progress: Arc<dyn SubagentProgressCallback> = Arc::clone(&recorder) as _;
        let bridge = SubagentEventBridge::new("test-agent".to_string(), progress);

        use crate::traits::AgentEventCallback;
        // These should not produce any events
        bridge.on_agent_chunk("hello");
        bridge.on_thinking("thought");
        bridge.on_critique("critique");
        bridge.on_thinking_refined("refined");

        assert!(recorder.events().is_empty());
    }
}
