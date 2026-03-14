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
use crate::main_agent::{MainAgent, MainAgentConfig};
use crate::react_loop::{ReactLoop, ReactLoopConfig};
use crate::traits::{AgentDeps, AgentError, AgentResult, BaseAgent, TaskMonitor};
use opendev_http::adapted_client::AdaptedClient;
use opendev_tools_core::ToolRegistry;

/// Well-known subagent types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubagentType {
    CodeExplorer,
    Planner,
    AskUser,
    PrReviewer,
    SecurityReviewer,
    WebClone,
    WebGenerator,
    Custom,
}

impl SubagentType {
    /// Parse a subagent type from a name string.
    pub fn from_name(name: &str) -> Self {
        match name {
            "Code-Explorer" | "code_explorer" => Self::CodeExplorer,
            "Planner" | "planner" => Self::Planner,
            "ask-user" | "ask_user" => Self::AskUser,
            "PR-Reviewer" | "pr_reviewer" => Self::PrReviewer,
            "Security-Reviewer" | "security_reviewer" => Self::SecurityReviewer,
            "Web-Clone" | "web_clone" => Self::WebClone,
            "Web-Generator" | "web_generator" => Self::WebGenerator,
            _ => Self::Custom,
        }
    }

    /// Get the canonical name for this type.
    pub fn canonical_name(&self) -> &'static str {
        match self {
            Self::CodeExplorer => "Code-Explorer",
            Self::Planner => "Planner",
            Self::AskUser => "ask-user",
            Self::PrReviewer => "PR-Reviewer",
            Self::SecurityReviewer => "Security-Reviewer",
            Self::WebClone => "Web-Clone",
            Self::WebGenerator => "Web-Generator",
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
    fn on_tool_call(&self, subagent_name: &str, tool_name: &str, tool_id: &str);

    /// Called when a subagent tool call completes.
    fn on_tool_complete(&self, subagent_name: &str, tool_name: &str, tool_id: &str, success: bool);

    /// Called when the subagent finishes (with or without error).
    fn on_finished(&self, subagent_name: &str, success: bool, result_summary: &str);
}

/// A no-op progress callback for when the caller doesn't need progress updates.
#[derive(Debug)]
pub struct NoopProgressCallback;

impl SubagentProgressCallback for NoopProgressCallback {
    fn on_started(&self, _name: &str, _task: &str) {}
    fn on_tool_call(&self, _name: &str, _tool: &str, _id: &str) {}
    fn on_tool_complete(&self, _name: &str, _tool: &str, _id: &str, _success: bool) {}
    fn on_finished(&self, _name: &str, _success: bool, _summary: &str) {}
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
    /// Additional subagents (PR-Reviewer, Security-Reviewer, Web-Clone, Web-Generator,
    /// ask-user) can be loaded as custom agents from `~/.opendev/agents/*.md`.
    pub fn with_builtins() -> Self {
        use super::spec::builtins;
        use crate::prompts::embedded;

        let mut mgr = Self::new();
        mgr.register(builtins::code_explorer(
            embedded::SUBAGENTS_SUBAGENT_CODE_EXPLORER,
        ));
        mgr.register(builtins::planner(embedded::SUBAGENTS_SUBAGENT_PLANNER));
        mgr.register(builtins::project_init(
            embedded::SUBAGENTS_SUBAGENT_PROJECT_INIT,
        ));
        mgr
    }

    /// Create a manager with built-in specs plus custom agents loaded from disk.
    ///
    /// Scans `{working_dir}/.opendev/agents/` and `~/.opendev/agents/` for
    /// user-defined agent markdown files. Custom agents override built-ins
    /// with the same name.
    pub fn with_builtins_and_custom(working_dir: &std::path::Path) -> Self {
        let mut mgr = Self::with_builtins();
        let dirs = vec![
            working_dir.join(".opendev").join("agents"),
            dirs::home_dir()
                .unwrap_or_default()
                .join(".opendev")
                .join("agents"),
        ];
        for spec in super::custom_loader::load_custom_agents(&dirs) {
            mgr.register(spec);
        }
        mgr
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

    /// List all registered subagent names.
    pub fn names(&self) -> Vec<&str> {
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

    /// Build tool schemas description listing available subagents.
    ///
    /// Used to populate the `subagent_type` enum in the `spawn_subagent` tool schema.
    pub fn build_enum_description(&self) -> Vec<(String, String)> {
        self.specs
            .values()
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
        progress: &dyn SubagentProgressCallback,
        task_monitor: Option<&dyn TaskMonitor>,
    ) -> Result<SubagentRunResult, AgentError> {
        let spec = self.get(subagent_name).ok_or_else(|| {
            AgentError::ConfigError(format!("Unknown subagent type: {subagent_name}"))
        })?;

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
        let allowed_tools = if spec.has_tool_restriction() {
            Some(spec.tools.clone())
        } else {
            None
        };

        // Create an isolated MainAgent for this subagent
        let config = MainAgentConfig {
            model,
            model_thinking: None,
            model_critique: None,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            working_dir: Some(working_dir.to_string()),
            allowed_tools,
            model_provider: None,
        };

        let mut agent = MainAgent::new(config, tool_registry);
        agent.set_http_client(http_client);
        agent.set_system_prompt(&spec.system_prompt);

        // Subagents get a limited iteration budget
        agent.set_react_config(ReactLoopConfig {
            max_iterations: Some(25),
            max_nudge_attempts: 2,
            max_todo_nudges: 2,
            ..Default::default()
        });

        debug!(subagent = %spec.name, "Running subagent ReAct loop");

        // Run the isolated ReAct loop
        let deps = AgentDeps::new();
        let result = agent.run(task, &deps, None, task_monitor).await;

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
        assert_eq!(SubagentType::from_name("ask-user"), SubagentType::AskUser);
        assert_eq!(SubagentType::from_name("unknown"), SubagentType::Custom);
    }

    #[test]
    fn test_subagent_type_canonical_name() {
        assert_eq!(SubagentType::CodeExplorer.canonical_name(), "Code-Explorer");
        assert_eq!(SubagentType::AskUser.canonical_name(), "ask-user");
    }

    #[test]
    fn test_with_builtins() {
        let mgr = SubagentManager::with_builtins();
        assert_eq!(mgr.len(), 3);
        assert!(mgr.get("Code-Explorer").is_some());
        assert!(mgr.get("Planner").is_some());
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
}
