//! Agents tool — list and spawn subagent types.
//!
//! Provides two tools:
//! - `agents` — List available subagent configurations
//! - `spawn_subagent` — Spawn a subagent to handle an isolated task
//!
//! Mirrors `opendev/core/context_engineering/tools/implementations/agents_tool.py`
//! and the subagent spawning logic from the Python react loop.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Built-in subagent type definition.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AgentType {
    name: String,
    description: String,
    tools: Vec<String>,
}

/// Tool for listing available subagent types.
#[derive(Debug)]
pub struct AgentsTool;

/// Default agent types available in the system.
fn default_agent_types() -> Vec<AgentType> {
    vec![
        AgentType {
            name: "code_explorer".into(),
            description: "Read-only agent for exploring and understanding codebases. \
                           Has access to file reading, search, and listing tools."
                .into(),
            tools: vec!["read_file".into(), "search".into(), "list_files".into()],
        },
        AgentType {
            name: "planner".into(),
            description: "Planning agent that creates implementation plans. \
                           Has read-only access to understand the codebase before planning."
                .into(),
            tools: vec![
                "read_file".into(),
                "search".into(),
                "list_files".into(),
                "write_file".into(),
            ],
        },
        AgentType {
            name: "ask_user".into(),
            description: "Agent that interacts with the user to gather information \
                           or clarify requirements."
                .into(),
            tools: vec!["ask_user".into()],
        },
    ]
}

#[async_trait::async_trait]
impl BaseTool for AgentsTool {
    fn name(&self) -> &str {
        "agents"
    }

    fn description(&self) -> &str {
        "List available subagent types with their descriptions and allowed tools."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform. Currently only 'list' is supported.",
                    "enum": ["list"]
                }
            }
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        match action {
            "list" => self.list_agents(ctx),
            other => ToolResult::fail(format!("Unknown action: {other}. Available actions: list")),
        }
    }
}

impl AgentsTool {
    fn list_agents(&self, ctx: &ToolContext) -> ToolResult {
        // Try to read agent types from context values (injected by runtime)
        let agents = if let Some(custom_agents) = ctx.values.get("agent_types") {
            match serde_json::from_value::<Vec<AgentType>>(custom_agents.clone()) {
                Ok(agents) => agents,
                Err(_) => default_agent_types(),
            }
        } else {
            default_agent_types()
        };

        if agents.is_empty() {
            return ToolResult::ok("No subagent types found.");
        }

        let mut parts = vec![format!("Available agents ({}):\n", agents.len())];

        for agent in &agents {
            parts.push(format!("  {}: {}", agent.name, agent.description));
            if !agent.tools.is_empty() {
                let tools_display: Vec<&str> =
                    agent.tools.iter().take(10).map(|s| s.as_str()).collect();
                parts.push(format!("    Tools: {}", tools_display.join(", ")));
            }
        }

        let output = parts.join("\n");

        let mut metadata = HashMap::new();
        metadata.insert(
            "agents".into(),
            serde_json::to_value(&agents).unwrap_or_default(),
        );
        metadata.insert("count".into(), serde_json::json!(agents.len()));

        ToolResult::ok_with_metadata(output, metadata)
    }
}

// ---------------------------------------------------------------------------
// SubagentEvent — typed messages sent from subagent back to parent/TUI
// ---------------------------------------------------------------------------

/// Events emitted by a running subagent, consumed by the parent agent or TUI.
#[derive(Debug, Clone)]
pub enum SubagentEvent {
    /// Subagent started.
    Started { subagent_name: String, task: String },
    /// Subagent made a tool call.
    ToolCall {
        subagent_name: String,
        tool_name: String,
        tool_id: String,
    },
    /// A subagent tool call completed.
    ToolComplete {
        subagent_name: String,
        tool_name: String,
        tool_id: String,
        success: bool,
    },
    /// Subagent finished.
    Finished {
        subagent_name: String,
        success: bool,
        result_summary: String,
        tool_call_count: usize,
        shallow_warning: Option<String>,
        total_tokens: u64,
    },
}

/// Progress callback that sends events through an mpsc channel.
///
/// Used to bridge subagent execution progress back to the TUI event loop.
pub struct ChannelProgressCallback {
    tx: mpsc::UnboundedSender<SubagentEvent>,
}

impl ChannelProgressCallback {
    /// Create a new channel-based progress callback.
    pub fn new(tx: mpsc::UnboundedSender<SubagentEvent>) -> Self {
        Self { tx }
    }
}

impl std::fmt::Debug for ChannelProgressCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelProgressCallback").finish()
    }
}

impl opendev_agents::SubagentProgressCallback for ChannelProgressCallback {
    fn on_started(&self, subagent_name: &str, task: &str) {
        let _ = self.tx.send(SubagentEvent::Started {
            subagent_name: subagent_name.to_string(),
            task: task.to_string(),
        });
    }

    fn on_tool_call(&self, subagent_name: &str, tool_name: &str, tool_id: &str) {
        let _ = self.tx.send(SubagentEvent::ToolCall {
            subagent_name: subagent_name.to_string(),
            tool_name: tool_name.to_string(),
            tool_id: tool_id.to_string(),
        });
    }

    fn on_tool_complete(&self, subagent_name: &str, tool_name: &str, tool_id: &str, success: bool) {
        let _ = self.tx.send(SubagentEvent::ToolComplete {
            subagent_name: subagent_name.to_string(),
            tool_name: tool_name.to_string(),
            tool_id: tool_id.to_string(),
            success,
        });
    }

    fn on_finished(
        &self,
        subagent_name: &str,
        success: bool,
        result_summary: &str,
        tool_call_count: usize,
        shallow_warning: Option<&str>,
        total_tokens: u64,
    ) {
        let _ = self.tx.send(SubagentEvent::Finished {
            subagent_name: subagent_name.to_string(),
            success,
            result_summary: result_summary.to_string(),
            tool_call_count,
            shallow_warning: shallow_warning.map(|s| s.to_string()),
            total_tokens,
        });
    }
}

// ---------------------------------------------------------------------------
// SpawnSubagentTool — the tool the LLM invokes to launch a subagent
// ---------------------------------------------------------------------------

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
        }
    }

    /// Set the event channel for progress reporting.
    pub fn with_event_sender(mut self, tx: mpsc::UnboundedSender<SubagentEvent>) -> Self {
        self.event_tx = Some(tx);
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
         use the tools directly instead. \
         IMPORTANT: Launch multiple agents concurrently whenever possible to maximize \
         performance. To do that, make ALL spawn_subagent calls in a SINGLE message \
         with multiple tool_use blocks. If you make them in separate messages, they \
         run sequentially which is much slower."
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
                                    Be specific about what the subagent should do, \
                                    which files to look at, and what output is expected."
                },
                "task_id": {
                    "type": "string",
                    "description": "Resume a previous subagent session by its task_id. \
                                    If provided, the subagent continues from where it left off."
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

        // Use working_dir from context if available, otherwise fall back to configured one
        let working_dir = ctx.working_dir.to_string_lossy().to_string();
        let wd = if working_dir.is_empty() || working_dir == "." {
            &self.working_dir
        } else {
            &working_dir
        };

        // Generate child session ID (reuse task_id for resume, new UUID otherwise)
        let child_session_id = task_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Create progress callback
        let progress: Box<dyn opendev_agents::SubagentProgressCallback> =
            if let Some(ref tx) = self.event_tx {
                Box::new(ChannelProgressCallback::new(tx.clone()))
            } else {
                Box::new(opendev_agents::NoopProgressCallback)
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
                wd,
                progress.as_ref(),
                None,
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

                let mut output = format!("task_id: {child_session_id} (for resuming)\n\n");

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

                // NOTE: The Finished event is already sent by manager.spawn() via
                // the progress callback. No duplicate send needed here.

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
    async fn test_list_agents_default() {
        let tool = AgentsTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("Available agents"));
        assert!(output.contains("code_explorer"));
        assert!(output.contains("planner"));
    }

    #[tokio::test]
    async fn test_list_agents_explicit_action() {
        let tool = AgentsTool;
        let ctx = ToolContext::new("/tmp");
        let mut args = HashMap::new();
        args.insert("action".to_string(), serde_json::json!("list"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_list_agents_unknown_action() {
        let tool = AgentsTool;
        let ctx = ToolContext::new("/tmp");
        let mut args = HashMap::new();
        args.insert("action".to_string(), serde_json::json!("spawn"));
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Unknown action"));
    }

    #[tokio::test]
    async fn test_list_agents_with_custom_context() {
        let tool = AgentsTool;
        let custom_agents = serde_json::json!([
            {
                "name": "custom_agent",
                "description": "A custom agent",
                "tools": ["read_file", "write_file"]
            }
        ]);
        let ctx = ToolContext::new("/tmp").with_value("agent_types", custom_agents);
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("custom_agent"));
        assert!(output.contains("A custom agent"));
    }

    #[test]
    fn test_default_agent_types() {
        let agents = default_agent_types();
        assert!(agents.len() >= 3);

        let names: Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"code_explorer"));
        assert!(names.contains(&"planner"));
        assert!(names.contains(&"ask_user"));
    }

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

    #[test]
    fn test_subagent_event_variants() {
        let started = SubagentEvent::Started {
            subagent_name: "Code-Explorer".into(),
            task: "Find all TODO comments".into(),
        };
        assert!(matches!(started, SubagentEvent::Started { .. }));

        let finished = SubagentEvent::Finished {
            subagent_name: "Code-Explorer".into(),
            success: true,
            result_summary: "Found 5 TODOs".into(),
            tool_call_count: 3,
            shallow_warning: None,
            total_tokens: 15000,
        };
        assert!(matches!(finished, SubagentEvent::Finished { .. }));
    }

    #[tokio::test]
    async fn test_channel_progress_callback() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let cb = ChannelProgressCallback::new(tx);

        use opendev_agents::SubagentProgressCallback;
        cb.on_started("test-agent", "do a thing");
        cb.on_tool_call("test-agent", "read_file", "tc-1");
        cb.on_tool_complete("test-agent", "read_file", "tc-1", true);
        cb.on_finished("test-agent", true, "Done", 3, None, 8000);

        let evt = rx.recv().await.unwrap();
        assert!(matches!(evt, SubagentEvent::Started { .. }));
        let evt = rx.recv().await.unwrap();
        assert!(matches!(evt, SubagentEvent::ToolCall { .. }));
        let evt = rx.recv().await.unwrap();
        assert!(matches!(evt, SubagentEvent::ToolComplete { .. }));
        let evt = rx.recv().await.unwrap();
        assert!(matches!(evt, SubagentEvent::Finished { .. }));
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
