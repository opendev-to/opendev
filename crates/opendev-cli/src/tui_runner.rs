//! Bridge between the ratatui TUI and the AgentRuntime.
//!
//! Spawns a background task that listens for user messages from the TUI,
//! runs them through the agent pipeline, and sends events back to update
//! the UI.

use std::io;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::info;

use opendev_agents::traits::AgentEventCallback;
use opendev_runtime::InterruptToken;
use opendev_tui::app::AppState;
use opendev_tui::{App, AppEvent};

use crate::runtime::AgentRuntime;

/// Event callback that forwards agent events to the TUI via AppEvent channel.
struct TuiEventCallback {
    tx: mpsc::UnboundedSender<AppEvent>,
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
    }

    fn on_tool_finished(&self, tool_id: &str, success: bool) {
        let _ = self.tx.send(AppEvent::ToolFinished {
            tool_id: tool_id.to_string(),
            success,
        });
    }

    fn on_tool_result(&self, tool_id: &str, tool_name: &str, output: &str, success: bool) {
        // Args will be looked up from the stored ToolExecution in app.rs
        let _ = self.tx.send(AppEvent::ToolResult {
            tool_id: tool_id.to_string(),
            tool_name: tool_name.to_string(),
            output: output.to_string(),
            success,
            args: std::collections::HashMap::new(),
        });
    }

    fn on_agent_chunk(&self, text: &str) {
        let _ = self.tx.send(AppEvent::AgentChunk(text.to_string()));
    }

    fn on_thinking(&self, content: &str) {
        let _ = self.tx.send(AppEvent::ThinkingTrace(content.to_string()));
    }

    fn on_critique(&self, content: &str) {
        let _ = self.tx.send(AppEvent::CritiqueTrace(content.to_string()));
    }

    fn on_thinking_refined(&self, content: &str) {
        let _ = self
            .tx
            .send(AppEvent::RefinedThinkingTrace(content.to_string()));
    }

    fn on_context_usage(&self, pct: f64) {
        let _ = self.tx.send(AppEvent::ContextUsage(pct));
    }
}

/// Bridges the TUI event loop with the AgentRuntime.
pub struct TuiRunner {
    runtime: AgentRuntime,
    system_prompt: String,
    initial_message: Option<String>,
}

impl TuiRunner {
    /// Create a new TUI runner.
    pub fn new(runtime: AgentRuntime, system_prompt: String) -> Self {
        Self {
            runtime,
            system_prompt,
            initial_message: None,
        }
    }

    /// Set an initial message to send to the agent when the TUI starts.
    pub fn with_initial_message(mut self, msg: Option<String>) -> Self {
        self.initial_message = msg;
        self
    }

    /// Run the TUI application with the agent backend.
    ///
    /// Sets up message forwarding between the TUI and the AgentRuntime,
    /// then runs the TUI event loop.
    pub async fn run(mut self, mut state: AppState) -> io::Result<()> {
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
                            } => AppEvent::SubagentStarted {
                                subagent_id,
                                subagent_name,
                                task,
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
        let callback = TuiEventCallback {
            tx: event_tx.clone(),
        };

        // Spawn the agent listener task
        let system_prompt = self.system_prompt;
        let mut runtime = self.runtime;

        tokio::spawn(async move {
            while let Some(msg) = user_rx.recv().await {
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

                info!(msg_len = msg.len(), "TUI: user submitted message");

                // Create fresh interrupt token for this query
                let interrupt_token = InterruptToken::new();
                let _ = event_tx.send(AppEvent::SetInterruptToken(interrupt_token.clone()));

                // Signal agent started
                let _ = event_tx.send(AppEvent::AgentStarted);
                let _ = event_tx.send(AppEvent::TaskProgressStarted {
                    description: "Thinking".to_string(),
                });

                // Run the query through the agent pipeline with event callback
                match runtime
                    .run_query(
                        &msg,
                        &system_prompt,
                        Some(&callback),
                        Some(&interrupt_token),
                    )
                    .await
                {
                    Ok(result) => {
                        let _ = event_tx.send(AppEvent::TaskProgressFinished);
                        if result.interrupted {
                            let _ = event_tx.send(AppEvent::AgentInterrupted);
                        } else {
                            let _ = event_tx.send(AppEvent::AgentFinished);
                            if !result.success {
                                let _ = event_tx.send(AppEvent::AgentError(
                                    "Agent completed with errors".to_string(),
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(AppEvent::TaskProgressFinished);
                        let _ = event_tx.send(AppEvent::AgentError(e.to_string()));
                    }
                }
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
