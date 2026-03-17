use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};
use tokio::sync::mpsc;
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
    /// Optional tool approval sender for bash/run_command approval.
    tool_approval_tx: Option<opendev_runtime::ToolApprovalSender>,
    /// Parent agent's max_tokens from model registry (subagents inherit this as fallback).
    parent_max_tokens: u64,
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
            tool_approval_tx: None,
            parent_max_tokens: 16384,
        }
    }

    /// Set the event channel for progress reporting.
    pub fn with_event_sender(mut self, tx: mpsc::UnboundedSender<SubagentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Set the tool approval sender for bash/run_command approval.
    pub fn with_tool_approval_tx(mut self, tx: opendev_runtime::ToolApprovalSender) -> Self {
        self.tool_approval_tx = Some(tx);
        self
    }

    /// Set the parent agent's max_tokens (subagents inherit this as fallback).
    pub fn with_parent_max_tokens(mut self, max_tokens: u64) -> Self {
        self.parent_max_tokens = max_tokens;
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

        // Create progress callback
        let progress: Arc<dyn opendev_agents::SubagentProgressCallback> =
            if let Some(ref tx) = self.event_tx {
                Arc::new(ChannelProgressCallback::new(
                    tx.clone(),
                    subagent_id.clone(),
                ))
            } else {
                Arc::new(opendev_agents::NoopProgressCallback)
            };

        // Spawn the subagent
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
                self.tool_approval_tx.as_ref(),
                self.parent_max_tokens,
            )
            .await;

        match result {
            Ok(run_result) => {
                // Save child session for future resume
                self.save_child_session(
                    &child_session_id,
                    agent_type,
                    task,
                    ctx.session_id.as_deref(),
                    &run_result,
                );

                // Embed stats as a parseable header so the TUI can extract them
                // even if bridge events (SubagentToolCall/Finished) haven't arrived yet.
                let mut output = format!(
                    "__subagent_stats__:tc={}\n",
                    run_result.tool_call_count
                );
                output.push_str(&format!("task_id: {child_session_id} (for resuming)\n\n"));

                // Cap subagent result size to prevent context bloat (50 KB max).
                const MAX_SUBAGENT_OUTPUT: usize = 50 * 1024;
                let content = &run_result.agent_result.content;
                if content.len() > MAX_SUBAGENT_OUTPUT {
                    let half = MAX_SUBAGENT_OUTPUT / 2;
                    output.push_str(&content[..half]);
                    output.push_str(&format!(
                        "\n\n[...truncated {} chars of subagent output...]\n\n",
                        content.len() - MAX_SUBAGENT_OUTPUT
                    ));
                    output.push_str(&content[content.len() - half..]);
                } else {
                    output.push_str(content);
                }

                // Append shallow subagent warning if applicable
                if let Some(ref warning) = run_result.shallow_warning {
                    output.push_str(warning);
                }

                // Send finished event with full details
                if let Some(ref tx) = self.event_tx {
                    let _ = tx.send(SubagentEvent::Finished {
                        subagent_id: subagent_id.clone(),
                        subagent_name: agent_type.to_string(),
                        success: run_result.agent_result.success,
                        result_summary: if output.len() > 200 {
                            format!("{}...", &output[..200])
                        } else {
                            output.clone()
                        },
                        tool_call_count: run_result.tool_call_count,
                        shallow_warning: run_result.shallow_warning.clone(),
                    });
                }

                if run_result.agent_result.success {
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "tool_call_count".into(),
                        serde_json::json!(run_result.tool_call_count),
                    );
                    metadata.insert("subagent_type".into(), serde_json::json!(agent_type));
                    metadata.insert("task_id".into(), serde_json::json!(child_session_id));
                    if run_result.agent_result.interrupted {
                        metadata.insert("interrupted".into(), serde_json::json!(true));
                    }
                    ToolResult::ok_with_metadata(output, metadata)
                } else if run_result.agent_result.interrupted {
                    ToolResult::fail("Subagent was interrupted by user")
                } else {
                    ToolResult::fail(format!("Subagent failed: {output}"))
                }
            }
            Err(e) => {
                warn!(agent_type = %agent_type, error = %e, "Subagent spawn failed");
                ToolResult::fail(format!("Failed to spawn subagent '{agent_type}': {e}"))
            }
        }
    }
}

impl SpawnSubagentTool {
    /// Save child session metadata to disk for future resume.
    fn save_child_session(
        &self,
        child_session_id: &str,
        agent_type: &str,
        task: &str,
        parent_session_id: Option<&str>,
        run_result: &opendev_agents::SubagentRunResult,
    ) {
        // Create a lightweight session manager for saving child sessions
        let child_mgr = match opendev_history::SessionManager::new(self.session_dir.clone()) {
            Ok(mgr) => mgr,
            Err(e) => {
                warn!(error = %e, "Failed to create session manager for child session");
                return;
            }
        };

        // Build a minimal session with the subagent result
        let mut session = opendev_models::session::Session::new();
        session.id = child_session_id.to_string();
        session.parent_id = parent_session_id.map(|s| s.to_string());
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

        // Convert agent result messages to ChatMessages
        let messages = opendev_history::message_convert::api_values_to_chatmessages(
            &run_result.agent_result.messages,
        );
        session.messages = messages;

        if let Err(e) = child_mgr.save_session(&session) {
            warn!(error = %e, "Failed to save child session");
        } else {
            info!(
                child_session_id = %child_session_id,
                parent_session_id = ?parent_session_id,
                "Saved child session"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_subagent_missing_params() {
        let manager = Arc::new(opendev_agents::SubagentManager::new());
        let registry = Arc::new(opendev_tools_core::ToolRegistry::new());
        let raw = opendev_http::HttpClient::new(
            "https://api.example.com/v1/chat/completions",
            reqwest::header::HeaderMap::new(),
            None,
        )
        .unwrap();
        let http = Arc::new(opendev_http::AdaptedClient::new(raw));
        let tool = SpawnSubagentTool::new(
            manager,
            registry,
            http,
            PathBuf::from("/tmp"),
            "gpt-4o",
            "/tmp",
        );
        let ctx = ToolContext::new("/tmp");

        // Missing agent_type
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("agent_type"));

        // Missing task
        let mut args = HashMap::new();
        args.insert("agent_type".into(), serde_json::json!("code_explorer"));
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("task"));
    }

    #[tokio::test]
    async fn test_spawn_subagent_unknown_type() {
        let manager = Arc::new(opendev_agents::SubagentManager::new());
        let registry = Arc::new(opendev_tools_core::ToolRegistry::new());
        let raw = opendev_http::HttpClient::new(
            "https://api.example.com/v1/chat/completions",
            reqwest::header::HeaderMap::new(),
            None,
        )
        .unwrap();
        let http = Arc::new(opendev_http::AdaptedClient::new(raw));
        let tool = SpawnSubagentTool::new(
            manager,
            registry,
            http,
            PathBuf::from("/tmp"),
            "gpt-4o",
            "/tmp",
        );
        let ctx = ToolContext::new("/tmp");

        let mut args = HashMap::new();
        args.insert("agent_type".into(), serde_json::json!("nonexistent"));
        args.insert("task".into(), serde_json::json!("do something"));
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Unknown subagent type"));
    }

    #[tokio::test]
    async fn test_spawn_subagent_blocked_in_subagent_context() {
        let manager = Arc::new(opendev_agents::SubagentManager::new());
        let registry = Arc::new(opendev_tools_core::ToolRegistry::new());
        let raw = opendev_http::HttpClient::new(
            "https://api.example.com/v1/chat/completions",
            reqwest::header::HeaderMap::new(),
            None,
        )
        .unwrap();
        let http = Arc::new(opendev_http::AdaptedClient::new(raw));
        let tool = SpawnSubagentTool::new(
            manager,
            registry,
            http,
            PathBuf::from("/tmp"),
            "gpt-4o",
            "/tmp",
        );

        // Simulate being called from within a subagent context
        let mut ctx = ToolContext::new("/tmp");
        ctx.is_subagent = true;

        let mut args = HashMap::new();
        args.insert("agent_type".into(), serde_json::json!("code_explorer"));
        args.insert("task".into(), serde_json::json!("explore code"));

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(
            result
                .error
                .unwrap()
                .contains("cannot spawn other subagents")
        );
    }
}
