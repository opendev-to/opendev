use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::events::{ChannelProgressCallback, SubagentEvent};

/// Tool that spawns and runs a subagent to handle an isolated task.
///
/// The LLM calls this tool with a subagent type and task description.
/// The tool creates an isolated agent with its own ReAct loop, runs it,
/// and returns the result back to the parent agent.
#[derive(Debug)]
pub struct SpawnSubagentTool {
    /// Subagent manager holding registered specs.
    manager: Arc<opendev_agents::SubagentManager>,
    /// Full tool registry (subagents filter to their allowed subset).
    tool_registry: Arc<opendev_tools_core::ToolRegistry>,
    /// HTTP client for LLM API calls.
    http_client: Arc<opendev_http::AdaptedClient>,
    /// Session directory for persisting child sessions.
    session_dir: PathBuf,
    /// Parent agent's model (used as fallback).
    parent_model: String,
    /// Working directory for tool execution.
    working_dir: String,
    /// Optional channel for sending progress events to the TUI.
    event_tx: Option<mpsc::UnboundedSender<SubagentEvent>>,
    /// Parent agent's max_tokens from model registry (subagents inherit this as fallback).
    parent_max_tokens: u64,
    /// Parent agent's reasoning effort (subagents inherit this).
    parent_reasoning_effort: Option<String>,
}

impl SpawnSubagentTool {
    /// Create a new spawn subagent tool.
    pub fn new(
        manager: Arc<opendev_agents::SubagentManager>,
        tool_registry: Arc<opendev_tools_core::ToolRegistry>,
        http_client: Arc<opendev_http::AdaptedClient>,
        session_dir: PathBuf,
        parent_model: impl Into<String>,
        working_dir: impl Into<String>,
    ) -> Self {
        Self {
            manager,
            tool_registry,
            http_client,
            session_dir,
            parent_model: parent_model.into(),
            working_dir: working_dir.into(),
            event_tx: None,
            parent_max_tokens: 16384,
            parent_reasoning_effort: None,
        }
    }

    /// Set the event channel for progress reporting.
    pub fn with_event_sender(mut self, tx: mpsc::UnboundedSender<SubagentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Set the parent agent's max_tokens (subagents inherit this as fallback).
    pub fn with_parent_max_tokens(mut self, max_tokens: u64) -> Self {
        self.parent_max_tokens = max_tokens;
        self
    }

    /// Set the parent agent's reasoning effort (subagents inherit this).
    pub fn with_parent_reasoning_effort(mut self, effort: Option<String>) -> Self {
        self.parent_reasoning_effort = effort;
        self
    }
}

#[async_trait::async_trait]
impl BaseTool for SpawnSubagentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Spawn a subagent to handle an isolated task. The subagent runs its own \
         ReAct loop with restricted tools and returns the result. Use for tasks \
         that require multiple tool calls and benefit from isolated context \
         (code exploration, summarization, codebase analysis, planning, web cloning, etc.). \
         This is the correct tool for 'summarize the codebase', 'how does X work', \
         'explore the code', etc. — NOT invoke_skill. \
         Do NOT spawn a subagent for tasks that only need 1-2 tool calls — \
         use the tools directly instead."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        // Build enum of available subagent types from manager
        let agent_names: Vec<String> = self.manager.names().iter().map(|s| s.to_string()).collect();

        serde_json::json!({
            "type": "object",
            "properties": {
                "agent_type": {
                    "type": "string",
                    "description": "The type of subagent to spawn.",
                    "enum": agent_names
                },
                "task": {
                    "type": "string",
                    "description": "Detailed task description for the subagent. \
                                    Be specific: which directories to explore, which patterns to search, \
                                    what questions to answer. When spawning multiple agents in parallel, \
                                    each task MUST be distinct — split by directory or question."
                },
                "task_id": {
                    "type": "string",
                    "description": "Resume a previous subagent session by its task_id. \
                                    If provided, the subagent continues from where it left off."
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for the subagent. Use this when the task \
                                    targets a different directory than the current project \
                                    (e.g., exploring another codebase at a specific path). \
                                    The subagent's tools will resolve relative paths from this directory."
                },
                "description": {
                    "type": "string",
                    "description": "A short (3-8 word) summary of the task for display. \
                                    Examples: 'Trace tool_call_count updates', 'Find auth middleware chain'."
                }
            },
            "required": ["agent_type", "task"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let agent_type = match args.get("agent_type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::fail("Missing required parameter: agent_type"),
        };

        let task = match args.get("task").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::fail("Missing required parameter: task"),
        };

        // Prevent recursive subagent spawning (subagents spawning subagents).
        if ctx.is_subagent {
            return ToolResult::fail(
                "Subagents cannot spawn other subagents. Complete your task directly \
                 using the tools available to you.",
            );
        }

        // Validate agent type exists before spawning background task
        if self.manager.get(agent_type).is_none() {
            return ToolResult::fail(format!("Unknown subagent type: {agent_type}"));
        }

        // Soft guard: block Planner spawn during explore phase
        let agent_type_lower = agent_type.to_lowercase();
        if agent_type_lower == "planner"
            && let Some(ref shared) = ctx.shared_state
            && let Ok(state) = shared.lock()
        {
            let phase = state
                .get("planning_phase")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if phase == "explore" {
                return ToolResult::fail(
                    "Before planning, first list the current directory structure \
                     and review relevant files to understand the codebase context. \
                     Use list_files, read_file, or search, then spawn Planner.",
                );
            }
        }

        let task_id = args.get("task_id").and_then(|v| v.as_str());

        info!(
            agent_type = %agent_type,
            task_len = task.len(),
            resume = task_id.is_some(),
            "spawn_subagent called"
        );

        // Pick raw path: explicit arg > context working_dir > configured default
        let wd = {
            let raw = if let Some(ewd) = args.get("working_dir").and_then(|v| v.as_str()) {
                std::path::PathBuf::from(ewd)
            } else if !ctx.working_dir.as_os_str().is_empty()
                && ctx.working_dir != std::path::Path::new(".")
            {
                ctx.working_dir.clone()
            } else {
                std::path::PathBuf::from(&self.working_dir)
            };

            // Resolve relative paths against configured default working directory
            let resolved = if raw.is_relative() {
                std::path::PathBuf::from(&self.working_dir).join(&raw)
            } else {
                raw
            };

            // Canonicalize to resolve symlinks and .. components
            match resolved.canonicalize() {
                Ok(p) if p.is_dir() => p.to_string_lossy().to_string(),
                Ok(p) => {
                    return ToolResult::fail(format!(
                        "working_dir '{}' is not a directory",
                        p.display()
                    ));
                }
                Err(_) => {
                    return ToolResult::fail(format!(
                        "working_dir '{}' does not exist or cannot be resolved",
                        resolved.display()
                    ));
                }
            }
        };

        // Generate child session ID (reuse task_id for resume, new UUID otherwise)
        let child_session_id = task_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Unique ID for this subagent instance (disambiguates parallel subagents)
        let subagent_id = uuid::Uuid::new_v4().to_string();

        // Create per-subagent child cancellation token
        let subagent_cancel = if let Some(parent) = ctx.cancel_token.as_ref() {
            parent.child_token()
        } else {
            CancellationToken::new()
        };

        // Create progress callback
        let progress: Arc<dyn opendev_agents::SubagentProgressCallback> =
            if let Some(ref tx) = self.event_tx {
                Arc::new(ChannelProgressCallback::new(
                    tx.clone(),
                    subagent_id.clone(),
                    Some(subagent_cancel.clone()),
                ))
            } else {
                Arc::new(opendev_agents::NoopProgressCallback)
            };

        // Execute subagent synchronously — blocking until it completes
        let result = self
            .manager
            .spawn(
                agent_type,
                task,
                &self.parent_model,
                Arc::clone(&self.tool_registry),
                Arc::clone(&self.http_client),
                &wd,
                progress,
                None,
                None,
                self.parent_max_tokens,
                self.parent_reasoning_effort.clone(),
                Some(subagent_cancel),
            )
            .await;

        match result {
            Ok(run_result) => {
                // Save child session for future resume
                let child_mgr = opendev_history::SessionManager::new(self.session_dir.clone());
                if let Ok(child_mgr) = child_mgr {
                    let mut session = opendev_models::session::Session::new();
                    session.id = child_session_id.clone();
                    session.parent_id = ctx.session_id.clone();
                    session.working_directory = Some(self.working_dir.clone());
                    session.metadata.insert(
                        "title".to_string(),
                        serde_json::json!(format!(
                            "{} (@{})",
                            task.chars().take(80).collect::<String>(),
                            agent_type
                        )),
                    );
                    session
                        .metadata
                        .insert("subagent_type".to_string(), serde_json::json!(agent_type));
                    let messages = opendev_history::message_convert::api_values_to_chatmessages(
                        &run_result.agent_result.messages,
                    );
                    session.messages = messages;
                    let _ = child_mgr.save_session(&session);
                }

                // Build output for injection
                let interrupted = run_result.agent_result.interrupted;
                let mut output = format!("__subagent_stats__:tc={}\n", run_result.tool_call_count);
                output.push_str(&format!("task_id: {child_session_id} (for resuming)\n\n"));
                if interrupted {
                    output.push_str("[WARNING: subagent was interrupted — result is partial]\n\n");
                }

                const MAX_SUBAGENT_OUTPUT: usize = 50 * 1024;
                let content = &run_result.agent_result.content;
                if content.len() > MAX_SUBAGENT_OUTPUT {
                    let half = MAX_SUBAGENT_OUTPUT / 2;
                    output.push_str(&format!(
                        "[WARNING: output truncated from {} to {} chars — result may be incomplete]\n\n",
                        content.len(),
                        MAX_SUBAGENT_OUTPUT
                    ));
                    output.push_str(opendev_runtime::safe_truncate(content, half));
                    output.push_str(&format!(
                        "\n\n[...truncated {} chars...]\n\n",
                        content.len() - MAX_SUBAGENT_OUTPUT
                    ));
                    // Take last `half` bytes, walking forward to a char boundary
                    let mut tail_start = content.len() - half;
                    while tail_start < content.len() && !content.is_char_boundary(tail_start) {
                        tail_start += 1;
                    }
                    output.push_str(&content[tail_start..]);
                } else {
                    output.push_str(content);
                }

                // Clean up markdown constructs that the TUI renderer can't handle
                output = clean_subagent_output(&output);

                if let Some(ref warning) = run_result.shallow_warning {
                    output.push_str(warning);
                }

                // Send finished event to TUI
                let effective_success = run_result.agent_result.success && !interrupted;
                if let Some(ref tx) = self.event_tx {
                    let _ = tx.send(SubagentEvent::Finished {
                        subagent_id: subagent_id.clone(),
                        subagent_name: agent_type.to_string(),
                        success: effective_success,
                        result_summary: if content.len() > 200 {
                            format!("{}...", opendev_runtime::safe_truncate(content, 200))
                        } else {
                            content.clone()
                        },
                        tool_call_count: run_result.tool_call_count,
                        shallow_warning: run_result.shallow_warning,
                    });
                }

                // Track explore subagent completion for planning phase transition
                if agent_type_lower == "explore"
                    && let Some(ref shared) = ctx.shared_state
                    && let Ok(mut state) = shared.lock()
                {
                    let count = state
                        .get("explore_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    state.insert("explore_count".into(), serde_json::json!(count + 1));
                    if state.get("planning_phase").and_then(|v| v.as_str()) == Some("explore") {
                        state.insert("planning_phase".into(), serde_json::json!("plan"));
                    }
                }

                ToolResult::ok(output)
            }
            Err(e) => {
                warn!(agent_type = %agent_type, error = %e, "Subagent failed");
                // Send finished event to TUI
                if let Some(ref tx) = self.event_tx {
                    let _ = tx.send(SubagentEvent::Finished {
                        subagent_id,
                        subagent_name: agent_type.to_string(),
                        success: false,
                        result_summary: e.to_string(),
                        tool_call_count: 0,
                        shallow_warning: None,
                    });
                }
                ToolResult::fail(format!("Subagent failed: {e}"))
            }
        }
    }
}

/// Clean subagent output by stripping markdown constructs that the TUI
/// renderer doesn't handle (horizontal rules, HTML tags, table syntax).
fn clean_subagent_output(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();
        // Strip horizontal rules (---, ***, ___)
        if (trimmed.starts_with("---") || trimmed.starts_with("***") || trimmed.starts_with("___"))
            && trimmed
                .chars()
                .all(|c| c == '-' || c == '*' || c == '_' || c == ' ')
            && trimmed.len() >= 3
        {
            result.push('\n');
            continue;
        }
        // Strip HTML tags (e.g. <br>, <div>, </div>)
        if trimmed.starts_with('<') && trimmed.ends_with('>') {
            continue;
        }
        // Simplify table rows: | col1 | col2 | → col1  col2
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            // Skip separator rows like |---|---|
            if trimmed.contains("---") {
                continue;
            }
            let cleaned: String = trimmed
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim())
                .collect::<Vec<_>>()
                .join("  ");
            result.push_str(&cleaned);
            result.push('\n');
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }
    // Remove trailing newline added by the loop
    if result.ends_with('\n') && !text.ends_with('\n') {
        result.pop();
    }
    result
}

#[cfg(test)]
#[path = "spawn_tests.rs"]
mod tests;
