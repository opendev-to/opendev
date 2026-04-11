//! Tool registration and system prompt construction.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use opendev_agents::prompts::create_default_composer;
use opendev_tools_core::ToolRegistry;
use opendev_tools_impl::*;

use super::ToolChannelReceivers;

/// Register all built-in tools into the registry.
pub(super) fn register_default_tools(
    registry: &ToolRegistry,
) -> (
    Arc<Mutex<opendev_runtime::TodoManager>>,
    ToolChannelReceivers,
    opendev_runtime::ToolApprovalSender,
) {
    registry.register_legacy_aliases();

    // Process execution
    registry.register(Arc::new(BashTool::new()));

    // File operations
    registry.register(Arc::new(FileReadTool));
    registry.register(Arc::new(FileWriteTool));
    registry.register(Arc::new(FileEditTool));
    registry.register(Arc::new(FileListTool));
    registry.register(Arc::new(GrepTool));

    // Web tools
    registry.register(Arc::new(WebFetchTool));
    registry.register(Arc::new(WebSearchTool));

    // User interaction — with channel for TUI mode
    let (ask_user_tx, ask_user_rx) = opendev_runtime::ask_user_channel();
    registry.register(Arc::new(AskUserTool::new().with_ask_tx(ask_user_tx)));

    // Memory
    registry.register(Arc::new(MemoryTool));

    // Scheduling & misc
    registry.register(Arc::new(ScheduleTool));
    registry.register(Arc::new(NotebookEditTool));
    registry.register(Arc::new(TaskCompleteTool));
    // Todo manager — created before plan tool so it can be shared
    let todo_manager = Arc::new(Mutex::new(opendev_runtime::TodoManager::new()));

    // Plan tool — with channel for TUI approval AND todo manager
    let (plan_approval_tx, plan_approval_rx) = opendev_runtime::plan_approval_channel();
    registry.register(Arc::new(
        PresentPlanTool::with_todo_manager(Arc::clone(&todo_manager))
            .with_approval_tx(plan_approval_tx),
    ));
    registry.register(Arc::new(WriteTodosTool::new(Arc::clone(&todo_manager))));
    registry.register(Arc::new(UpdateTodoTool::new(Arc::clone(&todo_manager))));
    registry.register(Arc::new(ListTodosTool::new(Arc::clone(&todo_manager))));
    // Note: SpawnSubagentTool requires shared Arc<ToolRegistry> and Arc<HttpClient>,
    // which are created after registration. Deferred for now.

    // Initialize TUI display map from tool metadata
    let display_map = registry.build_display_map();
    if !display_map.is_empty() {
        opendev_tui::formatters::tool_registry::init_runtime_display(display_map);
    }

    // Tool approval channel (sender stored on runtime for react loop, receiver goes to TUI)
    let (tool_approval_tx, tool_approval_rx) = opendev_runtime::tool_approval_channel();

    (
        todo_manager,
        ToolChannelReceivers {
            ask_user_rx,
            plan_approval_rx,
            tool_approval_rx,
            subagent_event_rx: None,
        },
        tool_approval_tx,
    )
}

/// Create a configured [`PromptComposer`] and [`PromptContext`] for
/// per-turn system prompt composition.
///
/// The composer includes all default sections plus a `Cached` dynamic
/// section for environment context (working directory, git status,
/// instruction files). MCP instructions should be registered separately
/// by the caller via [`PromptComposer::set_section_override`].
pub fn create_prompt_composer(
    working_dir: &Path,
    config: &opendev_models::AppConfig,
) -> (
    opendev_agents::prompts::PromptComposer,
    opendev_agents::prompts::PromptContext,
) {
    let mut composer = create_default_composer("/dev/null");

    // Register environment context as a Cached dynamic section.
    // This captures working dir, git status, instruction files, and model name.
    // Refreshed on /clear and /compact.
    let wd = working_dir.to_path_buf();
    let model_name = config.model.clone();
    let instructions = config.instructions.clone();
    composer.register_dynamic_section(
        "environment_context",
        opendev_agents::prompts::CachePolicy::Cached,
        92, // after code_references (90), before reminders_note (95)
        None,
        Box::new(move || {
            let mut env_ctx = opendev_context::EnvironmentContext::collect(&wd);
            env_ctx.model_name = Some(model_name.clone());

            // Resolve config-level instruction paths
            if !instructions.is_empty() {
                let config_instructions =
                    opendev_context::resolve_instruction_paths(&instructions, &wd);
                let existing: std::collections::HashSet<_> = env_ctx
                    .instruction_files
                    .iter()
                    .filter_map(|f| f.path.canonicalize().ok())
                    .collect();
                for instr in config_instructions {
                    if let Ok(canonical) = instr.path.canonicalize()
                        && !existing.contains(&canonical)
                    {
                        env_ctx.instruction_files.push(instr);
                    }
                }
            }

            let block = env_ctx.format_prompt_block();
            if block.is_empty() { None } else { Some(block) }
        }),
    );

    // Register MCP instructions as an Uncached section.
    // Content is injected via set_section_override() before each compose()
    // because MCP schema resolution is async. The section itself is a no-op
    // provider — the override mechanism supplies the actual content.
    composer.register_dynamic_section(
        "mcp_instructions",
        opendev_agents::prompts::CachePolicy::Uncached,
        78, // after task_tracking (75), before provider-specific (80)
        None,
        Box::new(|| None), // Content comes from set_section_override
    );

    let mut context = HashMap::new();
    context.insert(
        "model".to_string(),
        serde_json::Value::String(config.model.clone()),
    );
    context.insert(
        "working_dir".to_string(),
        serde_json::Value::String(working_dir.display().to_string()),
    );
    context.insert(
        "in_git_repo".to_string(),
        serde_json::Value::Bool(working_dir.join(".git").exists()),
    );
    context.insert("has_subagents".to_string(), serde_json::Value::Bool(true));
    context.insert("has_agent_teams".to_string(), serde_json::Value::Bool(true));
    context.insert(
        "todo_tracking_enabled".to_string(),
        serde_json::Value::Bool(true),
    );
    context.insert(
        "model_provider".to_string(),
        serde_json::Value::String(config.model_provider.clone()),
    );

    (composer, context)
}

/// Build the system prompt from embedded templates (convenience wrapper).
///
/// This composes once and returns a frozen string. For per-turn composition,
/// use [`create_prompt_composer`] instead.
#[cfg(test)]
pub fn build_system_prompt(working_dir: &Path, config: &opendev_models::AppConfig) -> String {
    let (mut composer, context) = create_prompt_composer(working_dir, config);
    composer.compose(&context)
}
