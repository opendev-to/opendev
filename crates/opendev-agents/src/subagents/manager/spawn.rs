//! Subagent spawning and runner selection.
//!
//! Contains the `spawn()` method on `SubagentManager` and the
//! `select_runner()` helper that picks the right runner based on agent type.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::SubagentManager;
use super::scanning::{scan_project_structure, scan_top_level_dirs};
use super::types::{SubagentEventBridge, SubagentRunResult, SubagentType};
use crate::react_loop::{ReactLoop, ReactLoopConfig};
use crate::subagents::spec::SubAgentSpec;
use crate::traits::{AgentError, TaskMonitor};
use opendev_http::adapted_client::AdaptedClient;
use opendev_tools_core::ToolRegistry;

/// Select the appropriate runner for a subagent based on its type.
fn select_runner(
    spec: &SubAgentSpec,
    _task: &str,
) -> Box<dyn super::super::runner::SubagentRunner> {
    use super::super::runner::{SimpleReactRunner, StandardReactRunner};

    match SubagentType::from_name(&spec.name) {
        SubagentType::CodeExplorer => {
            let max_iterations = spec.max_steps.unwrap_or(100) as usize;
            Box::new(SimpleReactRunner::new(
                max_iterations,
                std::time::Duration::from_secs(600),
            ))
        }
        _ => {
            // Default to 25 iterations for non-Explorer agents (matches old behavior)
            let max_iterations = Some(spec.max_steps.unwrap_or(25) as usize);
            Box::new(StandardReactRunner::new(ReactLoopConfig {
                max_iterations,
                max_nudge_attempts: 3,
                max_todo_nudges: 2,
                permission: spec.permission.clone(),
                ..Default::default()
            }))
        }
    }
}

impl SubagentManager {
    /// Spawn and run a subagent with the given task.
    ///
    /// Creates an isolated `MainAgent` with the subagent's restricted tool set,
    /// system prompt, and optional model override. Runs the subagent's own ReAct
    /// loop and returns the result along with diagnostic information.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn(
        &self,
        subagent_name: &str,
        task: &str,
        parent_model: &str,
        tool_registry: Arc<ToolRegistry>,
        http_client: Arc<AdaptedClient>,
        working_dir: &str,
        progress: Arc<dyn super::types::SubagentProgressCallback>,
        _task_monitor: Option<&dyn TaskMonitor>,
        tool_approval_tx: Option<&opendev_runtime::ToolApprovalSender>,
        parent_max_tokens: u64,
        parent_reasoning_effort: Option<String>,
        cancel_token: Option<CancellationToken>,
        debug_logger: Option<&opendev_runtime::SessionDebugLogger>,
        model_override: Option<&str>,
        mailbox: Option<&opendev_runtime::Mailbox>,
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
            working_dir = %working_dir,
            model_override = ?model_override,
            "Spawning subagent"
        );

        progress.on_started(&spec.name, task);

        // Determine model: model_override > spec.model > parent_model
        let model = model_override
            .or(spec.model.as_deref())
            .unwrap_or(parent_model)
            .to_string();

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
        // For Explore, the project structure is injected directly into
        // the system prompt (not a user message) for stronger grounding.
        let system_prompt = {
            let wd = std::path::Path::new(working_dir);
            let mut parts = vec![spec.system_prompt.clone()];

            // Always inject the working directory so the LLM knows the real path
            parts.push(format!(
                "\n\n## Working Directory\nYour working directory is: `{working_dir}`\nAll file paths should be relative to this directory or use this absolute path prefix. Do NOT use `/workspace/` or other Docker-style paths."
            ));

            // Inject project structure into system prompt for Explore
            if matches!(
                SubagentType::from_name(&spec.name),
                SubagentType::CodeExplorer
            ) {
                let structure = scan_project_structure(wd, 3);
                if !structure.is_empty() {
                    let top_dirs = scan_top_level_dirs(wd);
                    parts.push(format!(
                        "\n\n## Project Layout (auto-scanned)\n\
                         The following is the ACTUAL project structure. Use ONLY these paths.\n\n\
                         Top-level directories: {top_dirs}\n\n\
                         ```\n{structure}```\n\n\
                         CRITICAL: Paths like src/, lib/, app/ DO NOT exist unless listed above. \
                         Do NOT guess conventional paths — use the tree above or list_files to discover paths."
                    ));
                } else {
                    warn!(
                        subagent = %spec.name,
                        working_dir = %working_dir,
                        "Auto-scout: project structure scan returned empty — working directory may be invalid"
                    );
                }
            }

            let instructions = opendev_context::discover_instruction_files(wd);
            if !instructions.is_empty() {
                parts.push("\n\n# Project Instructions\n".to_string());
                for instr in &instructions {
                    let filename = instr.path.file_name().unwrap_or_default().to_string_lossy();
                    parts.push(format!(
                        "## {} ({})\n{}",
                        filename, instr.scope, instr.content
                    ));
                }
            }
            parts.join("\n")
        };

        // Build LlmCaller with subagent config
        let temperature = spec.temperature.map(|t| t as f64).unwrap_or(0.7);
        let llm_caller = crate::llm_calls::LlmCaller::new(crate::llm_calls::LlmCallConfig {
            model: model.clone(),
            temperature: Some(temperature),
            max_tokens: Some(spec.max_tokens.unwrap_or(parent_max_tokens as u32) as u64),
            // Subagents use lower reasoning effort to avoid excessive thinking.
            // Cap at "medium" — "high" parent becomes "medium" subagent;
            // "medium" and "low" pass through unchanged.
            reasoning_effort: parent_reasoning_effort
                .map(|e| if e == "high" { "medium".to_string() } else { e }),
        });

        // Build tool schemas (filtered to allowed tools)
        let tool_schemas = crate::main_agent::MainAgent::build_schemas_pub(
            &tool_registry,
            allowed_tools.as_deref(),
        );

        if tool_schemas.is_empty() {
            warn!(
                subagent = %spec.name,
                allowed_tools = ?allowed_tools,
                "Tool schemas are EMPTY — subagent has no tools available"
            );
        } else {
            debug!(
                subagent = %spec.name,
                schema_count = tool_schemas.len(),
                "Built tool schemas for subagent"
            );
        }

        // Build tool context
        let mut tool_context = opendev_tools_core::ToolContext::new(working_dir);
        tool_context.is_subagent = true;
        tool_context.cancel_token = cancel_token;

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

        // Note: project structure for Explore is now injected directly
        // into the system prompt above (stronger grounding than user messages).

        messages.push(serde_json::json!({"role": "user", "content": task}));

        // Build runner context
        let runner_ctx = super::super::runner::RunnerContext {
            caller: &llm_caller,
            http_client: &http_client,
            tool_schemas: &tool_schemas,
            tool_registry: &tool_registry,
            tool_context: &tool_context,
            event_callback: Some(bridge.as_ref() as &dyn crate::traits::AgentEventCallback),
            cancel: tool_context.cancel_token.as_ref(),
            tool_approval_tx,
            debug_logger,
            mailbox,
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
                    let end = agent_result.content.floor_char_boundary(200);
                    format!("{}...", &agent_result.content[..end])
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
