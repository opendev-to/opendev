//! Subagent manager for registering and executing subagents.
//!
//! Manages a collection of subagent specifications and provides
//! lookup by name or type. Also provides the execution entry point
//! for spawning subagents with isolated ReAct loops.

mod scanning;
mod spawn;
pub mod types;

pub use types::{
    NoopProgressCallback, SubagentEventBridge, SubagentProgressCallback, SubagentRunResult,
    SubagentType,
};

use std::collections::HashMap;

use tracing::info;

use super::spec::SubAgentSpec;

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
    /// Registers only the essential subagents (Explore, Planner, project_init).
    /// Additional subagents can be loaded as custom agents from `~/.opendev/agents/*.md`.
    pub fn with_builtins() -> Self {
        use super::spec::builtins;
        use crate::prompts::embedded;

        let mut mgr = Self::new();
        mgr.register(builtins::code_explorer(
            embedded::SUBAGENTS_SUBAGENT_CODE_EXPLORER,
        ));
        mgr.register(builtins::planner(embedded::SUBAGENTS_SUBAGENT_PLANNER));
        mgr.register(builtins::general(embedded::SUBAGENTS_SUBAGENT_GENERAL));
        mgr.register(builtins::build(embedded::SUBAGENTS_SUBAGENT_BUILD));
        mgr.register(builtins::verification(
            embedded::SUBAGENTS_SUBAGENT_VERIFICATION,
        ));
        mgr.register(builtins::project_init(
            embedded::SUBAGENTS_SUBAGENT_PROJECT_INIT,
        ));
        mgr
    }

    /// Create a manager with built-in specs plus custom agents loaded from disk.
    ///
    /// Scans agent directories from lowest to highest priority:
    /// 1. Global: `~/.opendev/agents/`
    /// 2. Walk from git root down to working_dir: `.opendev/agents/`
    ///    at each level (monorepo support)
    ///
    /// Custom agents override built-ins with the same name.
    /// Pass a pre-computed `git_root` to avoid a redundant `git rev-parse` call.
    pub fn with_builtins_and_custom(working_dir: &std::path::Path) -> Self {
        let git_root = crate::git_root(working_dir);
        Self::with_builtins_and_custom_inner(working_dir, git_root.as_deref())
    }

    /// Like [`with_builtins_and_custom`] but accepts a pre-resolved git root
    /// to avoid spawning a redundant `git rev-parse --show-toplevel` process.
    pub fn with_builtins_and_custom_git_root(
        working_dir: &std::path::Path,
        git_root: Option<&std::path::Path>,
    ) -> Self {
        Self::with_builtins_and_custom_inner(working_dir, git_root)
    }

    fn with_builtins_and_custom_inner(
        working_dir: &std::path::Path,
        git_root: Option<&std::path::Path>,
    ) -> Self {
        let mut mgr = Self::with_builtins();
        let home = dirs::home_dir().unwrap_or_default();

        let stop_dir = git_root.unwrap_or(working_dir);

        // Order: lowest priority first (later register() calls override earlier).
        let mut dirs = Vec::new();

        // 1. Global dir
        dirs.push(home.join(".opendev").join("agents"));

        // 2. Walk from working_dir up to git root, collect directory levels
        //    Parent dirs have lower priority than child dirs.
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
            dirs.push(level.join(".opendev").join("agents"));
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

    /// Build a human-readable agent listing for the LLM.
    ///
    /// Generates `- AgentName: description (Tools: tool1, tool2)` lines
    /// from live spec data. Sorted alphabetically for deterministic output.
    /// Excludes hidden and disabled agents.
    pub fn build_agent_listing(&self) -> String {
        let mut specs: Vec<&SubAgentSpec> = self
            .specs
            .values()
            .filter(|s| !s.hidden && !s.disable)
            .collect();
        specs.sort_by_key(|s| &s.name);

        let lines: Vec<String> = specs
            .iter()
            .map(|spec| {
                let tools_str = if spec.tools.is_empty() {
                    "all tools".to_string()
                } else {
                    spec.tools.join(", ")
                };
                format!(
                    "- {}: {} (Tools: {})",
                    spec.name, spec.description, tools_str
                )
            })
            .collect();
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests;
