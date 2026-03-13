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
use opendev_tui::app::AppState;
use opendev_tui::{App, AppEvent};

use crate::runtime::AgentRuntime;

/// Event callback that forwards agent events to the TUI via AppEvent channel.
struct TuiEventCallback {
    tx: mpsc::UnboundedSender<AppEvent>,
}

impl AgentEventCallback for TuiEventCallback {
    fn on_tool_started(&self, tool_id: &str, tool_name: &str) {
        let _ = self.tx.send(AppEvent::ToolStarted {
            tool_id: tool_id.to_string(),
            tool_name: tool_name.to_string(),
        });
    }

    fn on_tool_finished(&self, tool_id: &str, success: bool) {
        let _ = self.tx.send(AppEvent::ToolFinished {
            tool_id: tool_id.to_string(),
            success,
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
    pub async fn run(self, mut state: AppState) -> io::Result<()> {
        // Channel for forwarding user messages from TUI to agent task
        let (user_tx, mut user_rx) = mpsc::unbounded_channel::<String>();

        // Create the TUI app with the message channel
        let mut app = App::new().with_message_channel(user_tx.clone());

        // Apply initial state
        std::mem::swap(&mut app.state, &mut state);

        // Get event sender so the agent task can push UI updates
        let event_tx = app.event_sender();

        // Create the event callback for tool/agent events
        let callback = TuiEventCallback {
            tx: event_tx.clone(),
        };

        // Spawn the agent listener task
        let system_prompt = self.system_prompt;
        let mut runtime = self.runtime;

        tokio::spawn(async move {
            while let Some(msg) = user_rx.recv().await {
                info!(msg_len = msg.len(), "TUI: user submitted message");

                // Signal agent started
                let _ = event_tx.send(AppEvent::AgentStarted);
                let _ = event_tx.send(AppEvent::TaskProgressStarted {
                    description: "Thinking".to_string(),
                });

                // Run the query through the agent pipeline with event callback
                match runtime
                    .run_query(&msg, &system_prompt, Some(&callback))
                    .await
                {
                    Ok(result) => {
                        let _ = event_tx.send(AppEvent::TaskProgressFinished);
                        // The callback already sent AgentChunk for the final content.
                        // Just signal completion.
                        let _ = event_tx.send(AppEvent::AgentFinished);

                        if !result.success {
                            let _ = event_tx.send(AppEvent::AgentError(
                                "Agent completed with errors".to_string(),
                            ));
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
