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
    // Process execution
    registry.register(Arc::new(BashTool::new()));

    // File operations
    registry.register(Arc::new(FileReadTool));
    registry.register(Arc::new(FileWriteTool));
    registry.register(Arc::new(FileEditTool));
    registry.register(Arc::new(MultiEditTool));
    registry.register(Arc::new(FileListTool));
    registry.register(Arc::new(GrepTool));
    registry.register(Arc::new(AstGrepTool));

    // Git
    registry.register(Arc::new(GitTool));
    registry.register(Arc::new(PatchTool));

    // Web tools
    registry.register(Arc::new(WebFetchTool));
    registry.register(Arc::new(WebSearchTool));
    registry.register(Arc::new(WebScreenshotTool));
    registry.register(Arc::new(BrowserTool));
    registry.register(Arc::new(OpenBrowserTool));

    // User interaction — with channel for TUI mode
    let (ask_user_tx, ask_user_rx) = opendev_runtime::ask_user_channel();
    registry.register(Arc::new(AskUserTool::new().with_ask_tx(ask_user_tx)));

    // Memory & session
    registry.register(Arc::new(MemoryTool));
    registry.register(Arc::new(SessionTool));
    registry.register(Arc::new(MessageTool));

    // Scheduling & misc
    registry.register(Arc::new(ScheduleTool));
    // BatchTool is registered later (needs Arc<ToolRegistry> for dispatch).
    registry.register(Arc::new(NotebookEditTool));
    registry.register(Arc::new(TaskCompleteTool));
    registry.register(Arc::new(VlmTool));
    registry.register(Arc::new(DiffPreviewTool));
    // Plan tool — with channel for TUI approval
    let (plan_approval_tx, plan_approval_rx) = opendev_runtime::plan_approval_channel();
    registry.register(Arc::new(
        PresentPlanTool::new().with_approval_tx(plan_approval_tx),
    ));

    // Todo tools (5 separate tools sharing one manager)
    let todo_manager = Arc::new(Mutex::new(opendev_runtime::TodoManager::new()));
    registry.register(Arc::new(WriteTodosTool::new(Arc::clone(&todo_manager))));
    registry.register(Arc::new(UpdateTodoTool::new(Arc::clone(&todo_manager))));
    registry.register(Arc::new(CompleteTodoTool::new(Arc::clone(&todo_manager))));
    registry.register(Arc::new(ListTodosTool::new(Arc::clone(&todo_manager))));
    registry.register(Arc::new(ClearTodosTool::new(Arc::clone(&todo_manager))));
    // Keep legacy single-action tool for backward compatibility
    registry.register(Arc::new(TodoTool::new(Arc::clone(&todo_manager))));

    // Agent tools
    registry.register(Arc::new(AgentsTool));
    // Note: SpawnSubagentTool requires shared Arc<ToolRegistry> and Arc<HttpClient>,
    // which are created after registration. Deferred for now.

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

/// Build the system prompt from embedded templates.
pub fn build_system_prompt(working_dir: &Path, config: &opendev_models::AppConfig) -> String {
    // Use a dummy path — templates are resolved from the embedded store first
    let composer = create_default_composer("/dev/null");

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
        "is_git_repo".to_string(),
        serde_json::Value::Bool(working_dir.join(".git").exists()),
    );

    let base_prompt = composer.compose(&context);

    // Collect and append dynamic environment context
    let mut env_ctx = opendev_context::EnvironmentContext::collect(working_dir);

    // Resolve config-level instruction paths (file paths, globs, ~/paths)
    if !config.instructions.is_empty() {
        let config_instructions =
            opendev_context::resolve_instruction_paths(&config.instructions, working_dir);
        // Deduplicate against already-discovered files
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

    let env_block = env_ctx.format_prompt_block();

    if env_block.is_empty() {
        base_prompt
    } else {
        format!("{base_prompt}\n\n{env_block}")
    }
}
