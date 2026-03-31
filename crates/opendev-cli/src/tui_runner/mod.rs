//! Bridge between the ratatui TUI and the AgentRuntime.
//!
//! Spawns a background task that listens for user messages from the TUI,
//! runs them through the agent pipeline, and sends events back to update
//! the UI.

pub mod remote;

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{info, warn};

use chrono::Utc;
use opendev_agents::traits::AgentEventCallback;
use opendev_history::SessionManager;
use opendev_models::message::{ChatMessage, Role};
use opendev_runtime::InterruptToken;
use opendev_tui::app::AppState;
use opendev_tui::{App, AppEvent, ExitInfo};

use opendev_channels::telegram::remote::{RemoteCommandReceiver, RemoteEvent, RemoteEventSender};

use crate::runtime::AgentRuntime;

/// Event callback that forwards agent events to the TUI via AppEvent channel,
/// and optionally broadcasts to a remote session (Telegram).
struct TuiEventCallback {
    tx: mpsc::UnboundedSender<AppEvent>,
    remote_tx: Option<RemoteEventSender>,
}

impl AgentEventCallback for TuiEventCallback {
    fn on_tool_started(
        &self,
        tool_id: &str,
        tool_name: &str,
        args: &std::collections::HashMap<String, serde_json::Value>,
    ) {
        let _ = self.tx.send(AppEvent::ToolStarted {
            tool_id: tool_id.to_string(),
            tool_name: tool_name.to_string(),
            args: args.clone(),
        });
        if let Some(ref rtx) = self.remote_tx {
            let _ = rtx.send(RemoteEvent::ToolStarted {
                tool_name: tool_name.to_string(),
                args: args.clone(),
            });
        }
    }

    fn on_tool_finished(&self, tool_id: &str, success: bool) {
        let _ = self.tx.send(AppEvent::ToolFinished {
            tool_id: tool_id.to_string(),
            success,
        });
        // ToolFinished remote event is sent via on_tool_result for more info
    }

    fn on_tool_result(&self, tool_id: &str, tool_name: &str, output: &str, success: bool) {
        let _ = self.tx.send(AppEvent::ToolResult {
            tool_id: tool_id.to_string(),
            tool_name: tool_name.to_string(),
            output: output.to_string(),
            success,
            args: std::collections::HashMap::new(),
        });
        if let Some(ref rtx) = self.remote_tx {
            let _ = rtx.send(RemoteEvent::ToolResult {
                tool_name: tool_name.to_string(),
                output: output.to_string(),
                success,
            });
        }
    }

    fn on_agent_chunk(&self, text: &str) {
        let _ = self.tx.send(AppEvent::AgentChunk(text.to_string()));
        if let Some(ref rtx) = self.remote_tx {
            let _ = rtx.send(RemoteEvent::AgentChunk(text.to_string()));
        }
    }

    fn on_reasoning(&self, content: &str) {
        let _ = self
            .tx
            .send(AppEvent::ReasoningContent(content.to_string()));
    }

    fn on_reasoning_block_start(&self) {
        let _ = self.tx.send(AppEvent::ReasoningBlockStart);
    }

    fn on_context_usage(&self, pct: f64) {
        let _ = self.tx.send(AppEvent::ContextUsage(pct));
        if let Some(ref rtx) = self.remote_tx {
            let _ = rtx.send(RemoteEvent::ContextUsage(pct));
        }
    }

    fn on_file_changed(&self, files: usize, additions: u64, deletions: u64) {
        let _ = self.tx.send(AppEvent::FileChangeSummary {
            files,
            additions,
            deletions,
        });
        if let Some(ref rtx) = self.remote_tx {
            let _ = rtx.send(RemoteEvent::FileChangeSummary {
                files,
                additions,
                deletions,
            });
        }
    }
}

/// Event callback for background agent tasks.
///
/// Only emits `BackgroundAgentProgress` events — all other methods are no-ops
/// to prevent background tool events from leaking into the foreground display.
struct BackgroundEventCallback {
    tx: mpsc::UnboundedSender<AppEvent>,
    task_id: String,
    tool_count: Arc<AtomicUsize>,
}

impl AgentEventCallback for BackgroundEventCallback {
    fn on_tool_started(
        &self,
        _tool_id: &str,
        tool_name: &str,
        _args: &std::collections::HashMap<String, serde_json::Value>,
    ) {
        let count = self.tool_count.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.tx.send(AppEvent::BackgroundAgentProgress {
            task_id: self.task_id.clone(),
            tool_name: tool_name.to_string(),
            tool_count: count,
        });
        let _ = self.tx.send(AppEvent::BackgroundAgentActivity {
            task_id: self.task_id.clone(),
            line: format!("\u{25b8} {tool_name}"),
        });
    }

    fn on_tool_finished(&self, _tool_id: &str, _success: bool) {}

    fn on_tool_result(&self, _tool_id: &str, tool_name: &str, _output: &str, success: bool) {
        let icon = if success { "\u{2713}" } else { "\u{2717}" };
        let _ = self.tx.send(AppEvent::BackgroundAgentActivity {
            task_id: self.task_id.clone(),
            line: format!("  \u{23bf} {icon} {tool_name}"),
        });
    }

    fn on_agent_chunk(&self, _text: &str) {}

    fn on_reasoning(&self, content: &str) {
        let truncated: String = content.chars().take(80).collect();
        let _ = self.tx.send(AppEvent::BackgroundAgentActivity {
            task_id: self.task_id.clone(),
            line: format!("\u{27e1} {truncated}"),
        });
    }

    fn on_context_usage(&self, _pct: f64) {}
    fn on_file_changed(&self, _files: usize, _additions: u64, _deletions: u64) {}
}

/// Bridges the TUI event loop with the AgentRuntime.
pub struct TuiRunner {
    runtime: AgentRuntime,
    system_prompt: String,
    initial_message: Option<String>,
    /// Remote event sender for Telegram remote control.
    remote_event_tx: Option<RemoteEventSender>,
    /// Remote command receiver for Telegram remote control.
    remote_command_rx: Option<RemoteCommandReceiver>,
    /// Remote bridge for handling approval request resolution.
    remote_bridge: Option<Arc<opendev_channels::telegram::RemoteSessionBridge>>,
}

impl TuiRunner {
    /// Create a new TUI runner.
    pub fn new(runtime: AgentRuntime, system_prompt: String) -> Self {
        Self {
            runtime,
            system_prompt,
            initial_message: None,
            remote_event_tx: None,
            remote_command_rx: None,
            remote_bridge: None,
        }
    }

    /// Set an initial message to send to the agent when the TUI starts.
    pub fn with_initial_message(mut self, msg: Option<String>) -> Self {
        self.initial_message = msg;
        self
    }

    /// Attach a Telegram remote control session.
    pub fn with_remote_control(
        mut self,
        event_tx: RemoteEventSender,
        command_rx: RemoteCommandReceiver,
        bridge: Arc<opendev_channels::telegram::RemoteSessionBridge>,
    ) -> Self {
        self.remote_event_tx = Some(event_tx);
        self.remote_command_rx = Some(command_rx);
        self.remote_bridge = Some(bridge);
        self
    }

    /// Run the TUI application with the agent backend.
    ///
    /// Sets up message forwarding between the TUI and the AgentRuntime,
    /// then runs the TUI event loop.
    pub async fn run(mut self, mut state: AppState) -> io::Result<ExitInfo> {
        // Channel for forwarding user messages from TUI to agent task
        let (user_tx, mut user_rx) = mpsc::unbounded_channel::<String>();

        // Create the TUI app with the message channel
        let mut app = App::new().with_message_channel(user_tx.clone());

        // Apply initial state
        std::mem::swap(&mut app.state, &mut state);

        // Get event sender so the agent task can push UI updates
        let event_tx = app.event_sender();

        // Bridge channel receivers to AppEvents
        if let Some(receivers) = self.runtime.channel_receivers.take() {
            // Ask-user channel bridge
            let ask_tx = event_tx.clone();
            let mut ask_rx = receivers.ask_user_rx;
            tokio::spawn(async move {
                while let Some(req) = ask_rx.recv().await {
                    let _ = ask_tx.send(AppEvent::AskUserRequested {
                        question: req.question,
                        options: req.options,
                        default: req.default,
                        response_tx: req.response_tx,
                    });
                }
            });

            // Plan-approval channel bridge
            let plan_tx = event_tx.clone();
            let mut plan_rx = receivers.plan_approval_rx;
            tokio::spawn(async move {
                while let Some(req) = plan_rx.recv().await {
                    let _ = plan_tx.send(AppEvent::PlanApprovalRequested {
                        plan_content: req.plan_content,
                        response_tx: req.response_tx,
                    });
                }
            });

            // Tool-approval channel bridge
            let approval_tx = event_tx.clone();
            let mut tool_rx = receivers.tool_approval_rx;
            tokio::spawn(async move {
                while let Some(req) = tool_rx.recv().await {
                    let _ = approval_tx.send(AppEvent::ToolApprovalRequested {
                        command: req.command,
                        working_dir: req.working_dir,
                        response_tx: req.response_tx,
                    });
                }
            });

            // Subagent event channel bridge
            if let Some(mut subagent_rx) = receivers.subagent_event_rx {
                let sa_tx = event_tx.clone();
                tokio::spawn(async move {
                    use opendev_tools_impl::SubagentEvent;
                    while let Some(evt) = subagent_rx.recv().await {
                        let app_event = match evt {
                            SubagentEvent::Started {
                                subagent_id,
                                subagent_name,
                                task,
                                cancel_token,
                            } => AppEvent::SubagentStarted {
                                subagent_id,
                                subagent_name,
                                task,
                                cancel_token,
                            },
                            SubagentEvent::ToolCall {
                                subagent_id,
                                subagent_name,
                                tool_name,
                                tool_id,
                                args,
                            } => AppEvent::SubagentToolCall {
                                subagent_id,
                                subagent_name,
                                tool_name,
                                tool_id,
                                args,
                            },
                            SubagentEvent::ToolComplete {
                                subagent_id,
                                subagent_name,
                                tool_name,
                                tool_id,
                                success,
                            } => AppEvent::SubagentToolComplete {
                                subagent_id,
                                subagent_name,
                                tool_name,
                                tool_id,
                                success,
                            },
                            SubagentEvent::Finished {
                                subagent_id,
                                subagent_name,
                                success,
                                result_summary,
                                tool_call_count,
                                shallow_warning,
                            } => AppEvent::SubagentFinished {
                                subagent_id,
                                subagent_name,
                                success,
                                result_summary,
                                tool_call_count,
                                shallow_warning,
                            },
                            SubagentEvent::TokenUpdate {
                                subagent_id,
                                subagent_name,
                                input_tokens,
                                output_tokens,
                            } => AppEvent::SubagentTokenUpdate {
                                subagent_id,
                                subagent_name,
                                input_tokens,
                                output_tokens,
                            },
                        };
                        if sa_tx.send(app_event).is_err() {
                            break;
                        }
                    }
                });
            }
        }

        // Create the event callback for tool/agent events
        let remote_event_tx = self.remote_event_tx.take();
        let callback = TuiEventCallback {
            tx: event_tx.clone(),
            remote_tx: remote_event_tx.clone(),
        };

        // Spawn remote control bridges (Telegram → TUI)
        if let Some(remote_rx) = self.remote_command_rx.take() {
            remote::spawn_command_listener(remote_rx, user_tx.clone(), event_tx.clone());
        }

        if let (Some(bridge), Some(rtx)) = (self.remote_bridge.take(), remote_event_tx.clone()) {
            remote::spawn_approval_bridge(bridge, rtx, event_tx.clone(), &mut self.runtime);
        }

        // Spawn the agent listener task
        let system_prompt = self.system_prompt;
        let mut runtime = self.runtime;

        let remote_tx_for_agent = remote_event_tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = user_rx.recv().await {
                // Handle reasoning effort change sentinel
                if let Some(effort) = msg.strip_prefix("\x00__REASONING_EFFORT__") {
                    let new_effort = if effort == "none" {
                        None
                    } else {
                        Some(effort.to_string())
                    };
                    info!(effort = ?new_effort, "Reasoning effort changed");
                    runtime.llm_caller.config.reasoning_effort = new_effort;
                    continue;
                }

                // Handle model change sentinel
                if let Some(model) = msg.strip_prefix("\x00__MODEL_CHANGE__") {
                    match runtime.switch_model(model) {
                        Ok(name) => {
                            info!(model = %name, "Model switched via /models");
                        }
                        Err(e) => {
                            warn!(model = model, error = %e, "Model switch failed");
                            let _ = event_tx.send(AppEvent::AgentError(e));
                        }
                    }
                    continue;
                }

                // Handle undo sentinel
                if msg == "\x00__UNDO__" {
                    info!("TUI: undo requested");
                    let result = if let Ok(mut mgr) = runtime.snapshot_manager.lock() {
                        mgr.undo_last()
                    } else {
                        None
                    };
                    match result {
                        Some(desc) => {
                            let _ = event_tx.send(AppEvent::UndoResult {
                                success: true,
                                message: desc,
                            });
                        }
                        None => {
                            let _ = event_tx.send(AppEvent::UndoResult {
                                success: false,
                                message: "Nothing to undo.".to_string(),
                            });
                        }
                    }
                    continue;
                }

                // Handle redo sentinel
                if msg == "\x00__REDO__" {
                    info!("TUI: redo requested");
                    // Redo: re-track current state, then there's no built-in redo in SnapshotManager
                    // For now, show "not available"
                    let _ = event_tx.send(AppEvent::RedoResult {
                        success: false,
                        message: "Redo not yet available.".to_string(),
                    });
                    continue;
                }

                // Handle share sentinel
                if msg == "\x00__SHARE__" {
                    info!("TUI: share requested");
                    if let Some(session) = runtime.session_manager.current_session() {
                        match opendev_history::sharing::share_session(session, "").await {
                            Ok(url) => {
                                let _ = event_tx.send(AppEvent::ShareResult { path: url });
                            }
                            Err(e) => {
                                let _ = event_tx.send(AppEvent::AgentError(format!(
                                    "Failed to share session: {e}"
                                )));
                            }
                        }
                    } else {
                        let _ = event_tx.send(AppEvent::AgentError(
                            "No active session to share.".to_string(),
                        ));
                    }
                    continue;
                }

                // Handle list sessions sentinel
                if msg == "\x00__LIST_SESSIONS__" {
                    info!("TUI: list sessions requested");
                    let sessions = runtime.session_manager.list_sessions(false);
                    let mut lines = vec![format!("Sessions ({} total):", sessions.len())];
                    for s in sessions.iter().take(20) {
                        let title = s.title.as_deref().unwrap_or("(untitled)");
                        let date = s.updated_at.format("%Y-%m-%d %H:%M").to_string();
                        lines.push(format!(
                            "  {} — {} ({})",
                            &s.id[..8.min(s.id.len())],
                            title,
                            date
                        ));
                    }
                    let _ = event_tx.send(AppEvent::AgentError(lines.join("\n")));
                    continue;
                }

                // Handle manual compaction sentinel
                if msg == "\x00__COMPACT__" {
                    info!("TUI: manual compaction requested");
                    let _ = event_tx.send(AppEvent::CompactionStarted);

                    match runtime.run_compaction().await {
                        Ok(summary) => {
                            let _ = event_tx.send(AppEvent::CompactionFinished {
                                success: true,
                                message: summary,
                            });
                        }
                        Err(e) => {
                            let _ = event_tx.send(AppEvent::CompactionFinished {
                                success: false,
                                message: e,
                            });
                        }
                    }
                    continue;
                }

                // Handle background result injection sentinel
                if let Some(json_str) = msg.strip_prefix("\x00__BG_RESULT__") {
                    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(json_str) {
                        let task_id = payload["task_id"].as_str().unwrap_or("unknown");
                        let query = payload["query"].as_str().unwrap_or("");
                        let result = payload["result"].as_str().unwrap_or("");
                        let tool_call_count =
                            payload["tool_call_count"].as_u64().unwrap_or(0) as usize;

                        info!(task_id, tool_call_count, "Injecting background result as tool pair");

                        let interrupt_token = InterruptToken::new();
                        let _ =
                            event_tx.send(AppEvent::SetInterruptToken(interrupt_token.clone()));
                        let _ = event_tx.send(AppEvent::AgentStarted);
                        let _ = event_tx.send(AppEvent::TaskProgressStarted {
                            description: "Thinking".to_string(),
                        });

                        match runtime
                            .inject_background_result(
                                task_id,
                                query,
                                result,
                                tool_call_count,
                                &system_prompt,
                                Some(&callback),
                                Some(&interrupt_token),
                            )
                            .await
                        {
                            Ok(_agent_result) => {
                                let _ = event_tx.send(AppEvent::TaskProgressFinished);
                                let _ = event_tx.send(AppEvent::AgentFinished);
                            }
                            Err(e) => {
                                let _ = event_tx.send(AppEvent::TaskProgressFinished);
                                let _ = event_tx.send(AppEvent::AgentError(e.to_string()));
                            }
                        }
                    } else {
                        warn!("Failed to parse background result JSON");
                    }
                    continue;
                }

                // Detect and strip plan-mode sentinel
                let (msg, plan_requested) =
                    if let Some(stripped) = msg.strip_prefix("\x00__PLAN_MODE__") {
                        (stripped.to_string(), true)
                    } else {
                        (msg, false)
                    };

                info!(
                    msg_len = msg.len(),
                    plan_requested, "TUI: user submitted message"
                );

                // Create fresh interrupt token for this query
                let interrupt_token = InterruptToken::new();
                let _ = event_tx.send(AppEvent::SetInterruptToken(interrupt_token.clone()));

                // Signal agent started
                let _ = event_tx.send(AppEvent::AgentStarted);
                let _ = event_tx.send(AppEvent::TaskProgressStarted {
                    description: "Thinking".to_string(),
                });
                if let Some(ref rtx) = remote_tx_for_agent {
                    let _ = rtx.send(RemoteEvent::AgentStarted);
                }

                // Run the query through the agent pipeline with event callback
                match runtime
                    .run_query(
                        &msg,
                        &system_prompt,
                        Some(&callback),
                        Some(&interrupt_token),
                        plan_requested,
                    )
                    .await
                {
                    Ok(result) if result.backgrounded => {
                        let _ = event_tx.send(AppEvent::TaskProgressFinished);

                        // Generate short hex task ID
                        let task_id = format!(
                            "{:07x}",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .subsec_nanos()
                        );
                        let query_summary: String = msg.chars().take(60).collect();

                        // Save session to disk before forking — run_query skips save for backgrounded results
                        if let Err(e) = runtime.session_manager.save_current() {
                            warn!("Failed to save session before background fork: {e}");
                        }

                        // Fork session for background task
                        let forked_sm = if let Some(session) =
                            runtime.session_manager.current_session()
                        {
                            let session_id = session.id.clone();
                            match runtime.session_manager.fork_session(&session_id, None) {
                                Ok(forked_session) => {
                                    // Create a new SessionManager for the background task
                                    let session_dir =
                                        runtime.session_manager.session_dir().to_path_buf();
                                    match SessionManager::new(session_dir) {
                                        Ok(mut sm) => {
                                            sm.set_current_session(forked_session);
                                            Some(sm)
                                        }
                                        Err(e) => {
                                            warn!(
                                                "Failed to create background session manager: {e}"
                                            );
                                            None
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to fork session for background: {e}");
                                    None
                                }
                            }
                        } else {
                            None
                        };

                        let mut nudge_content: Option<String> = None;

                        // Save actual tool_call messages to FOREGROUND session
                        // (after fork, so the background runtime gets a clean session)
                        // This gives the LLM structural evidence (assistant tool_calls +
                        // tool results) that spawn_subagent was already called, preventing
                        // it from re-spawning the same agents on the next user message.
                        {
                            // Extract trailing assistant/tool block from result.messages
                            let new_start = result
                                .messages
                                .iter()
                                .rposition(|m| {
                                    let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
                                    role != "assistant" && role != "tool"
                                })
                                .map(|i| i + 1)
                                .unwrap_or(0);
                            let new_values = &result.messages[new_start..];

                            if !new_values.is_empty() {
                                let new_chat_messages =
                                    opendev_history::message_convert::api_values_to_chatmessages(
                                        new_values,
                                    );
                                for chat_msg in new_chat_messages {
                                    runtime.session_manager.add_message(chat_msg);
                                }

                                // Extract task descriptions for nudge prompt
                                let task_descs: Vec<String> = new_values
                                    .iter()
                                    .filter(|m| {
                                        m.get("role").and_then(|r| r.as_str()) == Some("tool")
                                            && m.get("name").and_then(|n| n.as_str())
                                                == Some("spawn_subagent")
                                    })
                                    .filter_map(|m| {
                                        m.get("content").and_then(|c| c.as_str()).map(String::from)
                                    })
                                    .collect();

                                if !task_descs.is_empty() {
                                    // Build minimal messages for nudge
                                    let nudge_messages = vec![
                                        serde_json::json!({
                                            "role": "system",
                                            "content": "You just delegated tasks to background agents. \
                                                        Write a brief, natural 1-2 sentence acknowledgment \
                                                        of what you delegated. Be concise. Do not use \
                                                        markdown. Do not use bullet points."
                                        }),
                                        serde_json::json!({
                                            "role": "user",
                                            "content": format!(
                                                "Original request: {msg}\n\n\
                                                 Delegated {} agents. Tool results:\n{}",
                                                task_descs.len(),
                                                task_descs.join("\n")
                                            )
                                        }),
                                    ];

                                    // Build payload — no tools, no reasoning (speed)
                                    let mut payload = runtime
                                        .llm_caller
                                        .build_action_payload(&nudge_messages, &[]);
                                    if let Some(obj) = payload.as_object_mut() {
                                        obj.remove("tool_choice");
                                        obj.remove("tools");
                                        obj.remove("_reasoning_effort");
                                        obj.insert(
                                            "max_tokens".to_string(),
                                            serde_json::json!(150),
                                        );
                                    }

                                    match runtime.http_client.post_json(&payload, None).await {
                                        Ok(http_result) if http_result.success => {
                                            if let Some(body) = http_result.body {
                                                let response =
                                                    runtime.llm_caller.parse_action_response(&body);

                                                // Track cost
                                                if let Some(ref usage_json) = response.usage {
                                                    let token_usage =
                                                        opendev_runtime::TokenUsage::from_json(
                                                            usage_json,
                                                        );
                                                    if let Ok(mut tracker) =
                                                        runtime.cost_tracker.lock()
                                                    {
                                                        tracker.record_usage(&token_usage, None);
                                                    }
                                                }

                                                if let Some(content) = response.content {
                                                    // Save to session
                                                    runtime.session_manager.add_message(
                                                        ChatMessage {
                                                            role: Role::Assistant,
                                                            content: content.clone(),
                                                            timestamp: Utc::now(),
                                                            metadata:
                                                                std::collections::HashMap::new(),
                                                            tool_calls: Vec::new(),
                                                            tokens: None,
                                                            thinking_trace: None,
                                                            reasoning_content: None,
                                                            token_usage: None,
                                                            provenance: None,
                                                        },
                                                    );

                                                    // Defer emission until after AgentBackgrounded
                                                    nudge_content = Some(content);
                                                }
                                            }
                                        }
                                        Ok(_) | Err(_) => {
                                            warn!("Background nudge LLM call failed, skipping");
                                        }
                                    }
                                }
                            } else {
                                // Fallback: no tool calls were made (e.g., backgrounded during LLM call)
                                runtime.session_manager.add_message(ChatMessage {
                                    role: Role::Assistant,
                                    content: format!(
                                        "The task was moved to a background agent which is \
                                         continuing the work independently.\n\
                                         Original request: {msg}"
                                    ),
                                    timestamp: Utc::now(),
                                    metadata: std::collections::HashMap::new(),
                                    tool_calls: Vec::new(),
                                    tokens: None,
                                    thinking_trace: None,
                                    reasoning_content: None,
                                    token_usage: None,
                                    provenance: None,
                                });
                            }
                            if let Err(e) = runtime.session_manager.save_current() {
                                warn!("Failed to save foreground session after backgrounding: {e}");
                            }
                        }

                        if let Some(bg_session_manager) = forked_sm {
                            let session_id = bg_session_manager
                                .current_session()
                                .map(|s| s.id.clone())
                                .unwrap_or_default();

                            // Create background runtime
                            match runtime.create_background_runtime(bg_session_manager) {
                                Ok(mut bg_runtime) => {
                                    // Fresh tokens for background task
                                    let bg_interrupt = InterruptToken::new();
                                    let bg_interrupt_for_mgr = bg_interrupt.clone();

                                    let bg_tx = event_tx.clone();
                                    let bg_task_id = task_id.clone();
                                    let bg_msg = msg.clone();
                                    let bg_system_prompt = system_prompt.clone();

                                    // Register in the manager via event
                                    let _ = event_tx.send(AppEvent::AgentBackgrounded {
                                        task_id: task_id.clone(),
                                        query_summary: query_summary.clone(),
                                    });

                                    // Emit nudge AFTER AgentBackgrounded so it renders below "Sent to background"
                                    if let Some(content) = nudge_content.take() {
                                        let _ =
                                            event_tx.send(AppEvent::BackgroundNudge { content });
                                    }

                                    // Spawn background task
                                    tokio::spawn(async move {
                                        let tool_count = Arc::new(AtomicUsize::new(0));
                                        let bg_callback = BackgroundEventCallback {
                                            tx: bg_tx.clone(),
                                            task_id: bg_task_id.clone(),
                                            tool_count: tool_count.clone(),
                                        };

                                        let result = bg_runtime
                                            .run_query(
                                                &bg_msg,
                                                &bg_system_prompt,
                                                Some(&bg_callback),
                                                Some(&bg_interrupt),
                                            )
                                            .await;

                                        let cost_usd = bg_runtime.total_cost_usd();
                                        let final_tool_count = tool_count.load(Ordering::Relaxed);

                                        match result {
                                            Ok(r) => {
                                                // Build rich result: LLM summary + raw subagent
                                                // outputs when the summary is too terse
                                                let full_result =
                                                    opendev_runtime::build_background_result(
                                                        &r.content,
                                                        &r.messages,
                                                        12000,
                                                    );
                                                let result_summary = if full_result.len() > 200 {
                                                    format!(
                                                        "{}...",
                                                        opendev_runtime::safe_truncate(
                                                            &full_result,
                                                            200,
                                                        )
                                                    )
                                                } else {
                                                    full_result.clone()
                                                };
                                                let _ = bg_tx.send(
                                                    AppEvent::BackgroundAgentCompleted {
                                                        task_id: bg_task_id,
                                                        success: r.success,
                                                        result_summary,
                                                        full_result,
                                                        cost_usd,
                                                        tool_call_count: final_tool_count,
                                                    },
                                                );
                                            }
                                            Err(e) => {
                                                let _ = bg_tx.send(
                                                    AppEvent::BackgroundAgentCompleted {
                                                        task_id: bg_task_id,
                                                        success: false,
                                                        result_summary: e.to_string(),
                                                        full_result: e.to_string(),
                                                        cost_usd,
                                                        tool_call_count: final_tool_count,
                                                    },
                                                );
                                            }
                                        }
                                    });

                                    // Register task in bg_agent_manager via a closure
                                    // (manager is in TUI state — we communicate via events)
                                    // The AgentBackgrounded event handler in event_dispatch
                                    // will display the message. We need to also register
                                    // the task — send it through the state.
                                    // We'll handle registration in event_dispatch.
                                    // For now, emit the token info so event_dispatch can register.
                                    // Actually, we need to register from TUI side.
                                    // Let's add a dedicated registration event.
                                    let _ = event_tx.send(AppEvent::SetBackgroundAgentToken {
                                        task_id: task_id.clone(),
                                        query: msg.clone(),
                                        session_id,
                                        interrupt_token: bg_interrupt_for_mgr,
                                    });
                                }
                                Err(e) => {
                                    warn!("Failed to create background runtime: {e}");
                                    let _ = event_tx.send(AppEvent::AgentError(format!(
                                        "Failed to background agent: {e}"
                                    )));
                                }
                            }
                        } else {
                            let _ = event_tx.send(AppEvent::AgentError(
                                "Failed to fork session for background agent.".to_string(),
                            ));
                        }

                        // Update session cost in TUI
                        if let Ok(tracker) = runtime.cost_tracker.lock() {
                            let _ = event_tx.send(AppEvent::CostUpdate(tracker.total_cost_usd));
                        }

                        // Free the foreground
                        let _ = event_tx.send(AppEvent::AgentFinished);
                    }
                    Ok(result) => {
                        let _ = event_tx.send(AppEvent::TaskProgressFinished);

                        // Emit snapshot for undo stack after successful query.
                        // Run on the blocking thread pool so the synchronous git
                        // operations (git add --all, git write-tree) don't starve
                        // the tokio runtime and freeze the TUI spinner.
                        if !result.interrupted {
                            let snap_mgr = runtime.snapshot_manager.clone();
                            let snap_tx = event_tx.clone();
                            tokio::task::spawn_blocking(move || {
                                if let Ok(mut mgr) = snap_mgr.lock()
                                    && let Some(hash) = mgr.track()
                                {
                                    let _ = snap_tx.send(AppEvent::SnapshotTaken { hash });
                                }
                            });
                        }

                        // Surface session title to TUI
                        if let Some(title) = runtime.session_manager.get_metadata("title") {
                            let _ = event_tx.send(AppEvent::SessionTitleUpdated(title));
                        }

                        if result.interrupted {
                            let _ = event_tx.send(AppEvent::AgentInterrupted);
                            if let Some(ref rtx) = remote_tx_for_agent {
                                let _ = rtx.send(RemoteEvent::AgentInterrupted);
                            }
                        } else {
                            let _ = event_tx.send(AppEvent::AgentFinished);
                            if let Some(ref rtx) = remote_tx_for_agent {
                                let _ = rtx.send(RemoteEvent::AgentFinished);
                            }
                            if !result.success {
                                let _ = event_tx.send(AppEvent::AgentError(
                                    "Agent completed with errors".to_string(),
                                ));
                            }
                        }

                        // Forward session title to remote
                        if let Some(ref rtx) = remote_tx_for_agent
                            && let Some(title) = runtime.session_manager.get_metadata("title")
                        {
                            let _ = rtx.send(RemoteEvent::SessionTitleUpdated(title));
                        }

                        // Update session cost in TUI
                        if let Ok(tracker) = runtime.cost_tracker.lock() {
                            let _ = event_tx.send(AppEvent::CostUpdate(tracker.total_cost_usd));
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(AppEvent::TaskProgressFinished);
                        let _ = event_tx.send(AppEvent::AgentError(e.to_string()));
                        if let Some(ref rtx) = remote_tx_for_agent {
                            let _ = rtx.send(RemoteEvent::AgentError(e.to_string()));
                        }

                        // Update session cost in TUI even on error
                        if let Ok(tracker) = runtime.cost_tracker.lock() {
                            let _ = event_tx.send(AppEvent::CostUpdate(tracker.total_cost_usd));
                        }
                    }
                }
            }

            // Save session on graceful exit (idempotent if run_query already saved)
            info!("TUI backend: saving session on exit");
            if let Err(e) = runtime.session_manager.save_current() {
                warn!("Failed to save session on exit: {e}");
            }
        });

        // If there's an initial message, inject it after a brief delay
        if let Some(msg) = self.initial_message {
            let init_tx = user_tx;
            tokio::spawn(async move {
                // Small delay to let the TUI initialize
                tokio::time::sleep(Duration::from_millis(100)).await;
                let _ = init_tx.send(msg);
            });
        }

        // Run the TUI (blocks until quit)
        app.run().await
    }
}
