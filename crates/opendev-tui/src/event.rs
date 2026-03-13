//! Event types for the TUI application.
//!
//! Bridges crossterm terminal events with application-level events
//! (agent messages, tool execution updates, etc.).

use crossterm::event::{Event as CrosstermEvent, KeyEvent, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

use opendev_models::message::ChatMessage;

/// Application-level events consumed by the main event loop.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Raw terminal event from crossterm.
    Terminal(CrosstermEvent),
    /// Key press (extracted from terminal event for convenience).
    Key(KeyEvent),
    /// Mouse event.
    Mouse(MouseEvent),
    /// Terminal resize.
    Resize(u16, u16),
    /// Tick for periodic UI updates (spinner animation, etc.).
    Tick,

    // -- Agent events --
    /// Assistant started generating a response.
    AgentStarted,
    /// Streaming text chunk from the assistant.
    AgentChunk(String),
    /// Complete assistant message received.
    AgentMessage(ChatMessage),
    /// Agent finished the current turn.
    AgentFinished,
    /// Agent encountered an error.
    AgentError(String),

    // -- Tool events --
    /// A tool execution started.
    ToolStarted {
        tool_id: String,
        tool_name: String,
    },
    /// A tool produced output.
    ToolOutput {
        tool_id: String,
        output: String,
    },
    /// A tool execution completed.
    ToolFinished {
        tool_id: String,
        success: bool,
    },
    /// Tool requires user approval.
    ToolApprovalRequired {
        tool_id: String,
        tool_name: String,
        description: String,
    },

    // -- Subagent events --
    /// A subagent started executing.
    SubagentStarted {
        subagent_name: String,
        task: String,
    },
    /// A subagent made a tool call (for nested display).
    SubagentToolCall {
        subagent_name: String,
        tool_name: String,
        tool_id: String,
    },
    /// A subagent tool call completed.
    SubagentToolComplete {
        subagent_name: String,
        tool_name: String,
        tool_id: String,
        success: bool,
    },
    /// A subagent finished its task.
    SubagentFinished {
        subagent_name: String,
        success: bool,
        result_summary: String,
        tool_call_count: usize,
        shallow_warning: Option<String>,
    },

    // -- Thinking events --
    /// A thinking trace was produced before the action phase.
    ThinkingTrace(String),
    /// A self-critique was produced (High thinking level only).
    CritiqueTrace(String),

    // -- Task progress events --
    /// Agent started working on a task (shows progress bar).
    TaskProgressStarted { description: String },
    /// Agent finished the current task (hides progress bar).
    TaskProgressFinished,

    // -- UI events --
    /// User submitted a message.
    UserSubmit(String),
    /// User requested interrupt (Escape).
    Interrupt,
    /// Mode changed (normal/plan).
    ModeChanged(String),
    /// Quit the application.
    Quit,
}

/// Handles crossterm event reading and dispatches [`AppEvent`]s.
pub struct EventHandler {
    /// Channel sender for emitting events.
    tx: mpsc::UnboundedSender<AppEvent>,
    /// Channel receiver for consuming events.
    rx: mpsc::UnboundedReceiver<AppEvent>,
    /// Tick rate for periodic updates.
    tick_rate: Duration,
}

impl EventHandler {
    /// Create a new event handler with the given tick rate.
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self { tx, rx, tick_rate }
    }

    /// Get a clone of the sender for external event producers (agent, tools).
    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.tx.clone()
    }

    /// Start the crossterm event reader loop.
    ///
    /// Uses crossterm's async `EventStream` for zero-latency event delivery
    /// instead of `spawn_blocking` + poll which adds up to 160ms delay.
    pub fn start(&self) {
        use futures::StreamExt;
        let tx = self.tx.clone();
        let tick_rate = self.tick_rate;

        tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            let mut tick_interval = tokio::time::interval(tick_rate);

            loop {
                let event = tokio::select! {
                    biased;
                    maybe_event = reader.next() => {
                        match maybe_event {
                            Some(Ok(CrosstermEvent::Key(key))) => AppEvent::Key(key),
                            Some(Ok(CrosstermEvent::Mouse(mouse))) => AppEvent::Mouse(mouse),
                            Some(Ok(CrosstermEvent::Resize(w, h))) => AppEvent::Resize(w, h),
                            Some(Ok(other)) => AppEvent::Terminal(other),
                            Some(Err(_)) => continue,
                            None => break, // stream ended
                        }
                    }
                    _ = tick_interval.tick() => AppEvent::Tick,
                };

                if tx.send(event).is_err() {
                    break;
                }
            }
        });
    }

    /// Receive the next event.
    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    /// Try to receive an event without blocking.
    /// Returns `None` immediately if no event is queued.
    pub fn try_next(&mut self) -> Option<AppEvent> {
        self.rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_handler_creation() {
        let handler = EventHandler::new(Duration::from_millis(250));
        let _sender = handler.sender();
    }

    #[tokio::test]
    async fn test_sender_delivers_events() {
        let mut handler = EventHandler::new(Duration::from_millis(250));
        let tx = handler.sender();
        tx.send(AppEvent::Tick).unwrap();
        let event = handler.next().await.unwrap();
        assert!(matches!(event, AppEvent::Tick));
    }

    #[tokio::test]
    async fn test_quit_event() {
        let mut handler = EventHandler::new(Duration::from_millis(250));
        let tx = handler.sender();
        tx.send(AppEvent::Quit).unwrap();
        let event = handler.next().await.unwrap();
        assert!(matches!(event, AppEvent::Quit));
    }
}
