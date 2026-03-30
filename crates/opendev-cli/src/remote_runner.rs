//! Headless remote runner for Telegram remote-control mode.
//!
//! Runs the agent without a TUI — Telegram is the primary interface.
//! Handles: message dispatch, tool approvals via bridge, streaming events,
//! and session management commands.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::info;

use opendev_agents::traits::AgentEventCallback;
use opendev_runtime::InterruptToken;

use opendev_channels::telegram::RemoteSessionBridge;
use opendev_channels::telegram::remote::{
    RemoteCommand, RemoteCommandReceiver, RemoteEvent, RemoteEventSender,
};

use crate::runtime::AgentRuntime;

/// Event callback that forwards agent events to the remote session (Telegram).
struct RemoteEventCallback {
    tx: RemoteEventSender,
}

impl AgentEventCallback for RemoteEventCallback {
    fn on_tool_started(
        &self,
        _tool_id: &str,
        tool_name: &str,
        args: &std::collections::HashMap<String, serde_json::Value>,
    ) {
        let _ = self.tx.send(RemoteEvent::ToolStarted {
            tool_name: tool_name.to_string(),
            args: args.clone(),
        });
    }

    fn on_tool_finished(&self, _tool_id: &str, _success: bool) {}

    fn on_tool_result(&self, _tool_id: &str, tool_name: &str, output: &str, success: bool) {
        let _ = self.tx.send(RemoteEvent::ToolResult {
            tool_name: tool_name.to_string(),
            output: output.to_string(),
            success,
        });
    }

    fn on_agent_chunk(&self, text: &str) {
        let _ = self.tx.send(RemoteEvent::AgentChunk(text.to_string()));
    }

    fn on_reasoning(&self, _content: &str) {}
    fn on_reasoning_block_start(&self) {}

    fn on_context_usage(&self, pct: f64) {
        let _ = self.tx.send(RemoteEvent::ContextUsage(pct));
    }

    fn on_file_changed(&self, files: usize, additions: u64, deletions: u64) {
        let _ = self.tx.send(RemoteEvent::FileChangeSummary {
            files,
            additions,
            deletions,
        });
    }
}

/// Run the headless remote session.
///
/// Blocks until Ctrl+C. Telegram is the only interface — no TUI.
pub async fn run_remote(
    mut runtime: AgentRuntime,
    system_prompt: String,
    event_tx: RemoteEventSender,
    mut command_rx: RemoteCommandReceiver,
    bridge: Arc<RemoteSessionBridge>,
) {
    // Connect MCP servers
    runtime.start_mcp_connections();

    let callback = RemoteEventCallback {
        tx: event_tx.clone(),
    };

    // Internal command type for the agent task
    enum AgentCmd {
        Message(String),
        NewSession,
        ResumeSession,
        Compact,
        Cost,
    }

    // Channel for forwarding commands to the agent task
    let (user_tx, mut user_rx) = mpsc::unbounded_channel::<AgentCmd>();

    // Bridge tool approval requests to Telegram
    if let Some(mut receivers) = runtime.channel_receivers.take() {
        let bridge_for_approval = Arc::clone(&bridge);
        let rtx_for_approval = event_tx.clone();

        // Intercept tool approval channel
        let mut original_approval_rx = std::mem::replace(&mut receivers.tool_approval_rx, {
            let (_tx, rx) = mpsc::unbounded_channel();
            rx
        });

        tokio::spawn(async move {
            while let Some(req) = original_approval_rx.recv().await {
                let request_id = bridge_for_approval.next_id();

                // Send to Telegram with inline keyboard
                let _ = rtx_for_approval.send(RemoteEvent::ToolApprovalNeeded {
                    request_id: request_id.clone(),
                    command: req.command.clone(),
                    working_dir: req.working_dir.clone(),
                });

                // Wait for Telegram response
                let remote_rx = bridge_for_approval.register_approval(&request_id).await;

                let original_command = req.command;
                match remote_rx.await {
                    Ok(response) => match response {
                        opendev_channels::telegram::remote::ApprovalResponse::Approved {
                            command,
                        } => {
                            let cmd = if command.is_empty() {
                                original_command
                            } else {
                                command
                            };
                            let _ = req.response_tx.send(opendev_runtime::ToolApprovalDecision {
                                approved: true,
                                choice: "yes".to_string(),
                                command: cmd,
                            });
                        }
                        opendev_channels::telegram::remote::ApprovalResponse::Denied => {
                            let _ = req.response_tx.send(opendev_runtime::ToolApprovalDecision {
                                approved: false,
                                choice: "no".to_string(),
                                command: original_command,
                            });
                        }
                    },
                    Err(_) => {
                        // Bridge closed / timeout — deny
                        let _ = req.response_tx.send(opendev_runtime::ToolApprovalDecision {
                            approved: false,
                            choice: "no".to_string(),
                            command: original_command,
                        });
                    }
                }
            }
        });

        // Bridge ask-user requests to Telegram
        let bridge_for_ask = Arc::clone(&bridge);
        let rtx_for_ask = event_tx.clone();
        let mut ask_rx = receivers.ask_user_rx;

        tokio::spawn(async move {
            while let Some(req) = ask_rx.recv().await {
                let request_id = bridge_for_ask.next_id();

                let _ = rtx_for_ask.send(RemoteEvent::AskUser {
                    request_id: request_id.clone(),
                    question: req.question.clone(),
                    options: req.options.clone(),
                });

                let answer_rx = bridge_for_ask.register_question(&request_id).await;

                match answer_rx.await {
                    Ok(answer) => {
                        let _ = req.response_tx.send(answer);
                    }
                    Err(_) => {
                        // Timeout — use default
                        let _ = req
                            .response_tx
                            .send(req.default.unwrap_or_else(|| "cancel".to_string()));
                    }
                }
            }
        });
    }

    // Spawn command listener (Telegram → agent)
    let cmd_user_tx = user_tx.clone();
    let cmd_interrupt: Arc<tokio::sync::Mutex<Option<InterruptToken>>> =
        Arc::new(tokio::sync::Mutex::new(None));
    let cmd_interrupt_clone = Arc::clone(&cmd_interrupt);

    tokio::spawn(async move {
        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                RemoteCommand::SendMessage(text) => {
                    let _ = cmd_user_tx.send(AgentCmd::Message(text));
                }
                RemoteCommand::Cancel => {
                    let token = cmd_interrupt_clone.lock().await;
                    if let Some(ref t) = *token {
                        t.request_interrupt();
                    }
                }
                RemoteCommand::NewSession => {
                    let _ = cmd_user_tx.send(AgentCmd::NewSession);
                }
                RemoteCommand::ResumeSession { .. } => {
                    let _ = cmd_user_tx.send(AgentCmd::ResumeSession);
                }
                RemoteCommand::Compact => {
                    let _ = cmd_user_tx.send(AgentCmd::Compact);
                }
                RemoteCommand::Cost => {
                    let _ = cmd_user_tx.send(AgentCmd::Cost);
                }
                // Approvals/questions handled by bridge directly
                RemoteCommand::ApproveToolCall { .. }
                | RemoteCommand::DenyToolCall { .. }
                | RemoteCommand::AnswerQuestion { .. } => {}
            }
        }
    });

    // Agent task: process messages from Telegram
    let event_tx_for_agent = event_tx.clone();
    let interrupt_store = Arc::clone(&cmd_interrupt);

    tokio::spawn(async move {
        while let Some(cmd) = user_rx.recv().await {
            match cmd {
                AgentCmd::Message(msg) => {
                    info!(msg_len = msg.len(), "Remote: user submitted message");

                    let interrupt_token = InterruptToken::new();
                    {
                        let mut store = interrupt_store.lock().await;
                        *store = Some(interrupt_token.clone());
                    }

                    let _ = event_tx_for_agent.send(RemoteEvent::AgentStarted);

                    match runtime
                        .run_query(
                            &msg,
                            &system_prompt,
                            Some(&callback),
                            Some(&interrupt_token),
                            false,
                        )
                        .await
                    {
                        Ok(result) => {
                            if result.interrupted {
                                let _ = event_tx_for_agent.send(RemoteEvent::AgentInterrupted);
                            } else {
                                let _ = event_tx_for_agent.send(RemoteEvent::AgentFinished);
                            }
                            if let Some(title) = runtime.session_manager.get_metadata("title") {
                                let _ = event_tx_for_agent
                                    .send(RemoteEvent::SessionTitleUpdated(title));
                            }
                        }
                        Err(e) => {
                            let _ = event_tx_for_agent.send(RemoteEvent::AgentError(e.to_string()));
                        }
                    }

                    {
                        let mut store = interrupt_store.lock().await;
                        *store = None;
                    }
                }
                AgentCmd::NewSession => {
                    runtime.session_manager.create_session();
                    let _ = runtime.session_manager.save_current();
                    let _ = event_tx_for_agent
                        .send(RemoteEvent::AgentError("New session started.".to_string()));
                }
                AgentCmd::ResumeSession => {
                    let sessions = runtime.session_manager.list_sessions(false);
                    if let Some(latest) = sessions.first() {
                        let id = latest.id.clone();
                        let title = latest
                            .title
                            .clone()
                            .unwrap_or_else(|| "(untitled)".to_string());
                        match runtime.session_manager.resume_session(&id) {
                            Ok(_) => {
                                let _ = event_tx_for_agent.send(RemoteEvent::AgentError(format!(
                                    "Resumed session: {title}"
                                )));
                            }
                            Err(e) => {
                                let _ = event_tx_for_agent.send(RemoteEvent::AgentError(format!(
                                    "Failed to resume: {e}"
                                )));
                            }
                        }
                    } else {
                        let _ = event_tx_for_agent.send(RemoteEvent::AgentError(
                            "No previous sessions found.".to_string(),
                        ));
                    }
                }
                AgentCmd::Compact => match runtime.run_compaction().await {
                    Ok(summary) => {
                        let _ = event_tx_for_agent.send(RemoteEvent::AgentError(summary));
                    }
                    Err(e) => {
                        let _ = event_tx_for_agent
                            .send(RemoteEvent::AgentError(format!("Compaction failed: {e}")));
                    }
                },
                AgentCmd::Cost => {
                    let cost = if let Ok(tracker) = runtime.cost_tracker.lock() {
                        format!(
                            "Session cost: {}\nAPI calls: {}\nInput tokens: {}\nOutput tokens: {}",
                            tracker.format_cost(),
                            tracker.call_count,
                            tracker.total_input_tokens,
                            tracker.total_output_tokens,
                        )
                    } else {
                        "Unable to read cost tracker.".to_string()
                    };
                    let _ = event_tx_for_agent.send(RemoteEvent::AgentError(cost));
                }
            }
        }
    });

    // Block until Ctrl+C
    println!("Remote session active. Interact via Telegram.");
    println!("Press Ctrl+C to stop.");
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for Ctrl+C");
    println!("\nShutting down...");
}
