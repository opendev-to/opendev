//! Event types for the TUI application.
//!
//! Bridges crossterm terminal events with application-level events
//! (agent messages, tool execution updates, etc.).

use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use opendev_runtime::InterruptToken;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use opendev_models::message::ChatMessage;

/// Application-level events consumed by the main event loop.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AppEvent {
    /// Raw terminal event from crossterm.
    Terminal(CrosstermEvent),
    /// Key press (extracted from terminal event for convenience).
    Key(KeyEvent),
    /// Terminal resize.
    Resize(u16, u16),
    /// Mouse-wheel scroll up (detected via arrow-key debounce from xterm alternate scroll).
    ScrollUp,
    /// Mouse-wheel scroll down (detected via arrow-key debounce from xterm alternate scroll).
    ScrollDown,
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
        args: std::collections::HashMap<String, serde_json::Value>,
    },
    /// A tool produced output.
    ToolOutput { tool_id: String, output: String },
    /// A tool produced its final result.
    ToolResult {
        tool_id: String,
        tool_name: String,
        output: String,
        success: bool,
        args: std::collections::HashMap<String, serde_json::Value>,
    },
    /// A tool execution completed.
    ToolFinished { tool_id: String, success: bool },
    /// Tool requires user approval (legacy, no channel — kept for recording compatibility).
    ToolApprovalRequired {
        tool_id: String,
        tool_name: String,
        description: String,
    },

    /// Tool approval request with bidirectional channel.
    ToolApprovalRequested {
        command: String,
        working_dir: String,
        response_tx: tokio::sync::oneshot::Sender<opendev_runtime::ToolApprovalDecision>,
    },

    /// Ask-user request with bidirectional channel.
    AskUserRequested {
        question: String,
        options: Vec<String>,
        default: Option<String>,
        response_tx: tokio::sync::oneshot::Sender<String>,
    },

    // -- Subagent events --
    /// A subagent started executing.
    SubagentStarted { subagent_name: String, task: String },
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
    /// A refined thinking trace was produced after critique (High thinking level only).
    RefinedThinkingTrace(String),

    // -- Task progress events --
    /// Agent started working on a task (shows progress bar).
    TaskProgressStarted { description: String },
    /// Agent finished the current task (hides progress bar).
    TaskProgressFinished,

    // -- Budget events --
    /// Session cost budget has been exhausted. The agent loop should pause.
    BudgetExhausted { cost_usd: f64, budget_usd: f64 },

    // -- Context events --
    /// Context window usage percentage updated (0.0–100.0).
    ContextUsage(f64),

    // -- Plan events --
    /// Plan approval request arrived from the PresentPlanTool.
    /// Contains the plan content to display and the oneshot sender for the decision.
    PlanApprovalRequested {
        plan_content: String,
        response_tx: tokio::sync::oneshot::Sender<opendev_runtime::PlanDecision>,
    },

    // -- UI events --
    /// User submitted a message.
    UserSubmit(String),
    /// User requested interrupt (Escape).
    Interrupt,
    /// Set the interrupt token for the current query (sent by agent backend).
    SetInterruptToken(InterruptToken),
    /// Agent run was interrupted (sent by agent backend after cancellation).
    AgentInterrupted,
    /// Mode changed (normal/plan).
    ModeChanged(String),
    /// Kill a background task by ID.
    KillTask(String),
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
    ///
    /// Includes a debounce state machine that distinguishes touchpad/mouse scroll
    /// (rapid-fire Up/Down arrows via xterm alternate scroll mode) from keyboard
    /// arrow presses. Touchpad scroll generates arrows every 8-16ms in bursts;
    /// keyboard presses are single events with ~300ms before repeat starts.
    /// A 25ms debounce window cleanly separates these two input sources.
    pub fn start(&self) {
        use futures::StreamExt;
        let tx = self.tx.clone();
        let tick_rate = self.tick_rate;

        tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            let mut tick_interval = tokio::time::interval(tick_rate);

            // Debounce state for distinguishing mouse scroll from keyboard arrows
            let debounce_window = Duration::from_millis(25);
            let scroll_burst_timeout = Duration::from_millis(100);
            let mut pending_arrow: Option<(KeyEvent, Instant)> = None;
            let mut scroll_burst = false;
            let mut last_arrow_time: Option<Instant> = None;

            loop {
                // Compute debounce deadline if we have a pending arrow
                let debounce_deadline = pending_arrow
                    .as_ref()
                    .map(|(_, t)| tokio::time::Instant::from_std(*t + debounce_window));

                tokio::select! {
                    biased;

                    // Debounce timer fires — pending arrow was a keyboard press
                    _ = async {
                        match debounce_deadline {
                            Some(deadline) => tokio::time::sleep_until(deadline).await,
                            None => std::future::pending().await,
                        }
                    } => {
                        if let Some((key, _)) = pending_arrow.take() {
                            scroll_burst = false;
                            if tx.send(AppEvent::Key(key)).is_err() {
                                break;
                            }
                        }
                    }

                    maybe_event = reader.next() => {
                        match maybe_event {
                            Some(Ok(CrosstermEvent::Key(key))) => {
                                let is_unmodified_arrow = matches!(
                                    key.code,
                                    KeyCode::Up | KeyCode::Down
                                ) && key.modifiers == KeyModifiers::NONE
                                  && key.kind == KeyEventKind::Press;

                                if is_unmodified_arrow {
                                    let now = Instant::now();

                                    // Check if we're in a scroll burst
                                    let in_burst = scroll_burst
                                        && last_arrow_time.is_some_and(|t| {
                                            now.duration_since(t) < scroll_burst_timeout
                                        });

                                    if let Some((prev_key, _)) = pending_arrow.take() {
                                        // Second arrow arrived within debounce window → mouse scroll
                                        scroll_burst = true;
                                        last_arrow_time = Some(now);
                                        let ev1 = if prev_key.code == KeyCode::Up {
                                            AppEvent::ScrollUp
                                        } else {
                                            AppEvent::ScrollDown
                                        };
                                        let ev2 = if key.code == KeyCode::Up {
                                            AppEvent::ScrollUp
                                        } else {
                                            AppEvent::ScrollDown
                                        };
                                        if tx.send(ev1).is_err() || tx.send(ev2).is_err() {
                                            break;
                                        }
                                    } else if in_burst {
                                        // Continuing a scroll burst
                                        last_arrow_time = Some(now);
                                        let ev = if key.code == KeyCode::Up {
                                            AppEvent::ScrollUp
                                        } else {
                                            AppEvent::ScrollDown
                                        };
                                        if tx.send(ev).is_err() {
                                            break;
                                        }
                                    } else {
                                        // First arrow — buffer it, wait for debounce
                                        pending_arrow = Some((key, now));
                                    }
                                } else {
                                    // Non-arrow key or arrow with modifiers/repeat:
                                    // flush any pending arrow as keyboard first
                                    if let Some((prev_key, _)) = pending_arrow.take() {
                                        scroll_burst = false;
                                        if tx.send(AppEvent::Key(prev_key)).is_err() {
                                            break;
                                        }
                                    }
                                    // Key repeat on arrows bypasses debounce (keyboard hold)
                                    if tx.send(AppEvent::Key(key)).is_err() {
                                        break;
                                    }
                                }
                            }
                            Some(Ok(CrosstermEvent::Mouse(_))) => continue,
                            Some(Ok(CrosstermEvent::Resize(w, h))) => {
                                // Flush pending arrow before resize
                                if let Some((prev_key, _)) = pending_arrow.take() {
                                    scroll_burst = false;
                                    if tx.send(AppEvent::Key(prev_key)).is_err() {
                                        break;
                                    }
                                }
                                if tx.send(AppEvent::Resize(w, h)).is_err() {
                                    break;
                                }
                            }
                            Some(Ok(other)) => {
                                // Flush pending arrow before other events
                                if let Some((prev_key, _)) = pending_arrow.take() {
                                    scroll_burst = false;
                                    if tx.send(AppEvent::Key(prev_key)).is_err() {
                                        break;
                                    }
                                }
                                if tx.send(AppEvent::Terminal(other)).is_err() {
                                    break;
                                }
                            }
                            Some(Err(_)) => continue,
                            None => break,
                        }
                    }

                    _ = tick_interval.tick() => {
                        // Don't flush pending arrow on tick — let debounce timer handle it
                        if tx.send(AppEvent::Tick).is_err() {
                            break;
                        }
                    }
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

// ---------------------------------------------------------------------------
// Serializable event representation for recording/replay (#98)
// ---------------------------------------------------------------------------

/// A serializable representation of [`AppEvent`] for JSONL recording and replay.
///
/// Terminal-level events (Key, Terminal, Resize) are recorded as debug strings
/// since crossterm types do not implement Serialize. Application-level events
/// are recorded with full fidelity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecordedEvent {
    /// Monotonic sequence number.
    pub seq: u64,
    /// Timestamp in milliseconds since the recorder was created.
    pub timestamp_ms: u64,
    /// The event variant name (e.g., "AgentStarted", "ToolResult").
    pub variant: String,
    /// Serialized event payload. For terminal events this is a debug string;
    /// for application events this contains structured data.
    pub payload: serde_json::Value,
}

impl RecordedEvent {
    /// Create a `RecordedEvent` from an `AppEvent`.
    fn from_app_event(event: &AppEvent, seq: u64, elapsed_ms: u64) -> Self {
        let (variant, payload) = match event {
            AppEvent::Terminal(e) => ("Terminal".to_string(), serde_json::json!(format!("{e:?}"))),
            AppEvent::Key(k) => ("Key".to_string(), serde_json::json!(format!("{k:?}"))),
            AppEvent::Resize(w, h) => ("Resize".to_string(), serde_json::json!({"w": w, "h": h})),
            AppEvent::ScrollUp => ("ScrollUp".to_string(), serde_json::Value::Null),
            AppEvent::ScrollDown => ("ScrollDown".to_string(), serde_json::Value::Null),
            AppEvent::Tick => ("Tick".to_string(), serde_json::Value::Null),
            AppEvent::AgentStarted => ("AgentStarted".to_string(), serde_json::Value::Null),
            AppEvent::AgentChunk(s) => ("AgentChunk".to_string(), serde_json::json!({"chunk": s})),
            AppEvent::AgentMessage(msg) => (
                "AgentMessage".to_string(),
                serde_json::to_value(msg).unwrap_or(serde_json::Value::Null),
            ),
            AppEvent::AgentFinished => ("AgentFinished".to_string(), serde_json::Value::Null),
            AppEvent::AgentError(e) => ("AgentError".to_string(), serde_json::json!({"error": e})),
            AppEvent::ToolStarted {
                tool_id,
                tool_name,
                args,
            } => (
                "ToolStarted".to_string(),
                serde_json::json!({"tool_id": tool_id, "tool_name": tool_name, "args": args}),
            ),
            AppEvent::ToolOutput { tool_id, output } => (
                "ToolOutput".to_string(),
                serde_json::json!({"tool_id": tool_id, "output": output}),
            ),
            AppEvent::ToolResult {
                tool_id,
                tool_name,
                output,
                success,
                args,
            } => (
                "ToolResult".to_string(),
                serde_json::json!({
                    "tool_id": tool_id,
                    "tool_name": tool_name,
                    "output": output,
                    "success": success,
                    "args": args,
                }),
            ),
            AppEvent::ToolFinished { tool_id, success } => (
                "ToolFinished".to_string(),
                serde_json::json!({"tool_id": tool_id, "success": success}),
            ),
            AppEvent::ToolApprovalRequired {
                tool_id,
                tool_name,
                description,
            } => (
                "ToolApprovalRequired".to_string(),
                serde_json::json!({
                    "tool_id": tool_id,
                    "tool_name": tool_name,
                    "description": description,
                }),
            ),
            AppEvent::SubagentStarted {
                subagent_name,
                task,
            } => (
                "SubagentStarted".to_string(),
                serde_json::json!({"subagent_name": subagent_name, "task": task}),
            ),
            AppEvent::SubagentToolCall {
                subagent_name,
                tool_name,
                tool_id,
            } => (
                "SubagentToolCall".to_string(),
                serde_json::json!({
                    "subagent_name": subagent_name,
                    "tool_name": tool_name,
                    "tool_id": tool_id,
                }),
            ),
            AppEvent::SubagentToolComplete {
                subagent_name,
                tool_name,
                tool_id,
                success,
            } => (
                "SubagentToolComplete".to_string(),
                serde_json::json!({
                    "subagent_name": subagent_name,
                    "tool_name": tool_name,
                    "tool_id": tool_id,
                    "success": success,
                }),
            ),
            AppEvent::SubagentFinished {
                subagent_name,
                success,
                result_summary,
                tool_call_count,
                shallow_warning,
            } => (
                "SubagentFinished".to_string(),
                serde_json::json!({
                    "subagent_name": subagent_name,
                    "success": success,
                    "result_summary": result_summary,
                    "tool_call_count": tool_call_count,
                    "shallow_warning": shallow_warning,
                }),
            ),
            AppEvent::ThinkingTrace(s) => {
                ("ThinkingTrace".to_string(), serde_json::json!({"trace": s}))
            }
            AppEvent::CritiqueTrace(s) => {
                ("CritiqueTrace".to_string(), serde_json::json!({"trace": s}))
            }
            AppEvent::RefinedThinkingTrace(s) => (
                "RefinedThinkingTrace".to_string(),
                serde_json::json!({"trace": s}),
            ),
            AppEvent::TaskProgressStarted { description } => (
                "TaskProgressStarted".to_string(),
                serde_json::json!({"description": description}),
            ),
            AppEvent::TaskProgressFinished => {
                ("TaskProgressFinished".to_string(), serde_json::Value::Null)
            }
            AppEvent::BudgetExhausted {
                cost_usd,
                budget_usd,
            } => (
                "BudgetExhausted".to_string(),
                serde_json::json!({"cost_usd": cost_usd, "budget_usd": budget_usd}),
            ),
            AppEvent::ContextUsage(pct) => {
                ("ContextUsage".to_string(), serde_json::json!({"pct": pct}))
            }
            AppEvent::ToolApprovalRequested {
                command,
                working_dir,
                ..
            } => (
                "ToolApprovalRequested".to_string(),
                serde_json::json!({"command": command, "working_dir": working_dir}),
            ),
            AppEvent::AskUserRequested {
                question,
                options,
                default,
                ..
            } => (
                "AskUserRequested".to_string(),
                serde_json::json!({"question": question, "options": options, "default": default}),
            ),
            AppEvent::PlanApprovalRequested { plan_content, .. } => (
                "PlanApprovalRequested".to_string(),
                serde_json::json!({"plan_content": plan_content}),
            ),
            AppEvent::UserSubmit(s) => {
                ("UserSubmit".to_string(), serde_json::json!({"message": s}))
            }
            AppEvent::Interrupt => ("Interrupt".to_string(), serde_json::Value::Null),
            AppEvent::SetInterruptToken(_) => {
                ("SetInterruptToken".to_string(), serde_json::Value::Null)
            }
            AppEvent::AgentInterrupted => ("AgentInterrupted".to_string(), serde_json::Value::Null),
            AppEvent::ModeChanged(m) => ("ModeChanged".to_string(), serde_json::json!({"mode": m})),
            AppEvent::KillTask(id) => ("KillTask".to_string(), serde_json::json!({"task_id": id})),
            AppEvent::Quit => ("Quit".to_string(), serde_json::Value::Null),
        };

        RecordedEvent {
            seq,
            timestamp_ms: elapsed_ms,
            variant,
            payload,
        }
    }

    /// Try to reconstruct an `AppEvent` from a recorded event.
    ///
    /// Terminal/Key events cannot be reconstructed and return `None`.
    /// All application-level events are reconstructed with full fidelity.
    pub fn to_app_event(&self) -> Option<AppEvent> {
        match self.variant.as_str() {
            "Tick" => Some(AppEvent::Tick),
            "AgentStarted" => Some(AppEvent::AgentStarted),
            "AgentChunk" => {
                let chunk = self.payload.get("chunk")?.as_str()?.to_string();
                Some(AppEvent::AgentChunk(chunk))
            }
            "AgentMessage" => {
                let msg: ChatMessage = serde_json::from_value(self.payload.clone()).ok()?;
                Some(AppEvent::AgentMessage(msg))
            }
            "AgentFinished" => Some(AppEvent::AgentFinished),
            "AgentError" => {
                let error = self.payload.get("error")?.as_str()?.to_string();
                Some(AppEvent::AgentError(error))
            }
            "ToolStarted" => {
                let tool_id = self.payload.get("tool_id")?.as_str()?.to_string();
                let tool_name = self.payload.get("tool_name")?.as_str()?.to_string();
                let args: std::collections::HashMap<String, serde_json::Value> =
                    serde_json::from_value(self.payload.get("args")?.clone()).ok()?;
                Some(AppEvent::ToolStarted {
                    tool_id,
                    tool_name,
                    args,
                })
            }
            "ToolOutput" => {
                let tool_id = self.payload.get("tool_id")?.as_str()?.to_string();
                let output = self.payload.get("output")?.as_str()?.to_string();
                Some(AppEvent::ToolOutput { tool_id, output })
            }
            "ToolResult" => {
                let tool_id = self.payload.get("tool_id")?.as_str()?.to_string();
                let tool_name = self.payload.get("tool_name")?.as_str()?.to_string();
                let output = self.payload.get("output")?.as_str()?.to_string();
                let success = self.payload.get("success")?.as_bool()?;
                let args: std::collections::HashMap<String, serde_json::Value> =
                    serde_json::from_value(self.payload.get("args")?.clone()).ok()?;
                Some(AppEvent::ToolResult {
                    tool_id,
                    tool_name,
                    output,
                    success,
                    args,
                })
            }
            "ToolFinished" => {
                let tool_id = self.payload.get("tool_id")?.as_str()?.to_string();
                let success = self.payload.get("success")?.as_bool()?;
                Some(AppEvent::ToolFinished { tool_id, success })
            }
            "ToolApprovalRequired" => {
                let tool_id = self.payload.get("tool_id")?.as_str()?.to_string();
                let tool_name = self.payload.get("tool_name")?.as_str()?.to_string();
                let description = self.payload.get("description")?.as_str()?.to_string();
                Some(AppEvent::ToolApprovalRequired {
                    tool_id,
                    tool_name,
                    description,
                })
            }
            "SubagentStarted" => {
                let subagent_name = self.payload.get("subagent_name")?.as_str()?.to_string();
                let task = self.payload.get("task")?.as_str()?.to_string();
                Some(AppEvent::SubagentStarted {
                    subagent_name,
                    task,
                })
            }
            "SubagentToolCall" => {
                let subagent_name = self.payload.get("subagent_name")?.as_str()?.to_string();
                let tool_name = self.payload.get("tool_name")?.as_str()?.to_string();
                let tool_id = self.payload.get("tool_id")?.as_str()?.to_string();
                Some(AppEvent::SubagentToolCall {
                    subagent_name,
                    tool_name,
                    tool_id,
                })
            }
            "SubagentToolComplete" => {
                let subagent_name = self.payload.get("subagent_name")?.as_str()?.to_string();
                let tool_name = self.payload.get("tool_name")?.as_str()?.to_string();
                let tool_id = self.payload.get("tool_id")?.as_str()?.to_string();
                let success = self.payload.get("success")?.as_bool()?;
                Some(AppEvent::SubagentToolComplete {
                    subagent_name,
                    tool_name,
                    tool_id,
                    success,
                })
            }
            "SubagentFinished" => {
                let subagent_name = self.payload.get("subagent_name")?.as_str()?.to_string();
                let success = self.payload.get("success")?.as_bool()?;
                let result_summary = self.payload.get("result_summary")?.as_str()?.to_string();
                let tool_call_count = self.payload.get("tool_call_count")?.as_u64()? as usize;
                let shallow_warning = self
                    .payload
                    .get("shallow_warning")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                Some(AppEvent::SubagentFinished {
                    subagent_name,
                    success,
                    result_summary,
                    tool_call_count,
                    shallow_warning,
                })
            }
            "ThinkingTrace" => {
                let trace = self.payload.get("trace")?.as_str()?.to_string();
                Some(AppEvent::ThinkingTrace(trace))
            }
            "CritiqueTrace" => {
                let trace = self.payload.get("trace")?.as_str()?.to_string();
                Some(AppEvent::CritiqueTrace(trace))
            }
            "RefinedThinkingTrace" => {
                let trace = self.payload.get("trace")?.as_str()?.to_string();
                Some(AppEvent::RefinedThinkingTrace(trace))
            }
            "TaskProgressStarted" => {
                let description = self.payload.get("description")?.as_str()?.to_string();
                Some(AppEvent::TaskProgressStarted { description })
            }
            "TaskProgressFinished" => Some(AppEvent::TaskProgressFinished),
            "BudgetExhausted" => {
                let cost_usd = self.payload.get("cost_usd")?.as_f64()?;
                let budget_usd = self.payload.get("budget_usd")?.as_f64()?;
                Some(AppEvent::BudgetExhausted {
                    cost_usd,
                    budget_usd,
                })
            }
            "ContextUsage" => {
                let pct = self.payload.get("pct")?.as_f64()?;
                Some(AppEvent::ContextUsage(pct))
            }
            // These cannot be reconstructed (contain oneshot senders)
            "PlanApprovalRequested" => None,
            "ToolApprovalRequested" => None,
            "AskUserRequested" => None,
            "UserSubmit" => {
                let message = self.payload.get("message")?.as_str()?.to_string();
                Some(AppEvent::UserSubmit(message))
            }
            "Interrupt" => Some(AppEvent::Interrupt),
            // SetInterruptToken and AgentInterrupted cannot be reconstructed
            "SetInterruptToken" => None,
            "AgentInterrupted" => Some(AppEvent::AgentInterrupted),
            "ModeChanged" => {
                let mode = self.payload.get("mode")?.as_str()?.to_string();
                Some(AppEvent::ModeChanged(mode))
            }
            "KillTask" => {
                let task_id = self.payload.get("task_id")?.as_str()?.to_string();
                Some(AppEvent::KillTask(task_id))
            }
            "Quit" => Some(AppEvent::Quit),
            "ScrollUp" => Some(AppEvent::ScrollUp),
            "ScrollDown" => Some(AppEvent::ScrollDown),
            // Terminal/Key/Resize cannot be reconstructed
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// EventRecorder — records AppEvents to a JSONL file (#98)
// ---------------------------------------------------------------------------

/// Records all [`AppEvent`] variants to a JSONL file for debugging and replay.
///
/// Activated when the `OPENDEV_DEBUG_EVENTS=1` environment variable is set.
/// Each event is serialized as a single JSON line with a sequence number and
/// timestamp for deterministic replay.
pub struct EventRecorder {
    file: std::io::BufWriter<std::fs::File>,
    seq: u64,
    start: std::time::Instant,
}

impl EventRecorder {
    /// Create a new recorder that writes to the given path.
    ///
    /// Returns `None` if the file cannot be created.
    pub fn new(path: &Path) -> Option<Self> {
        let file = std::fs::File::create(path).ok()?;
        Some(Self {
            file: std::io::BufWriter::new(file),
            seq: 0,
            start: std::time::Instant::now(),
        })
    }

    /// Create a recorder if `OPENDEV_DEBUG_EVENTS=1` is set.
    ///
    /// Writes to `~/.opendev/debug/events-<timestamp>.jsonl`.
    pub fn from_env() -> Option<Self> {
        if std::env::var("OPENDEV_DEBUG_EVENTS").ok()?.as_str() != "1" {
            return None;
        }
        let home = dirs::home_dir()?;
        let debug_dir = home.join(".opendev").join("debug");
        std::fs::create_dir_all(&debug_dir).ok()?;
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let path = debug_dir.join(format!("events-{timestamp}.jsonl"));
        tracing::info!(path = %path.display(), "Event recording enabled");
        Self::new(&path)
    }

    /// Record an event. Silently ignores write errors.
    pub fn record(&mut self, event: &AppEvent) {
        self.seq += 1;
        let elapsed = self.start.elapsed().as_millis() as u64;
        let recorded = RecordedEvent::from_app_event(event, self.seq, elapsed);
        if let Ok(json) = serde_json::to_string(&recorded) {
            let _ = writeln!(self.file, "{json}");
            let _ = self.file.flush();
        }
    }

    /// Return the output file path (for logging).
    pub fn path(&self) -> Option<PathBuf> {
        // Path is not stored, but callers typically know it.
        None
    }
}

/// Load recorded events from a JSONL file for replay.
///
/// Returns events in sequence order. Terminal/Key events that cannot
/// be reconstructed are skipped.
pub fn load_recorded_events(path: &Path) -> std::io::Result<Vec<RecordedEvent>> {
    let content = std::fs::read_to_string(path)?;
    let mut events = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<RecordedEvent>(line) {
            Ok(event) => events.push(event),
            Err(e) => {
                tracing::warn!(error = %e, "Skipping malformed event line");
            }
        }
    }
    events.sort_by_key(|e| e.seq);
    Ok(events)
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

    #[test]
    fn test_event_recorder_roundtrip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        // Record some events
        {
            let mut recorder = EventRecorder::new(&path).unwrap();
            recorder.record(&AppEvent::AgentStarted);
            recorder.record(&AppEvent::AgentChunk("hello".to_string()));
            recorder.record(&AppEvent::ToolStarted {
                tool_id: "t1".to_string(),
                tool_name: "bash".to_string(),
                args: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("command".to_string(), serde_json::json!("echo hi"));
                    m
                },
            });
            recorder.record(&AppEvent::AgentFinished);
            recorder.record(&AppEvent::Quit);
        }

        // Load and verify
        let events = load_recorded_events(&path).unwrap();
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].variant, "AgentStarted");
        assert_eq!(events[1].variant, "AgentChunk");
        assert_eq!(events[2].variant, "ToolStarted");
        assert_eq!(events[3].variant, "AgentFinished");
        assert_eq!(events[4].variant, "Quit");

        // Verify reconstruction
        assert!(matches!(
            events[0].to_app_event().unwrap(),
            AppEvent::AgentStarted
        ));
        assert!(matches!(
            events[1].to_app_event().unwrap(),
            AppEvent::AgentChunk(_)
        ));
        assert!(matches!(events[4].to_app_event().unwrap(), AppEvent::Quit));
    }

    #[test]
    fn test_recorded_event_sequence_numbers() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let mut recorder = EventRecorder::new(&path).unwrap();
        recorder.record(&AppEvent::Tick);
        recorder.record(&AppEvent::Tick);
        recorder.record(&AppEvent::Tick);
        drop(recorder);

        let events = load_recorded_events(&path).unwrap();
        assert_eq!(events[0].seq, 1);
        assert_eq!(events[1].seq, 2);
        assert_eq!(events[2].seq, 3);
        // Timestamps should be monotonically non-decreasing
        assert!(events[1].timestamp_ms >= events[0].timestamp_ms);
        assert!(events[2].timestamp_ms >= events[1].timestamp_ms);
    }

    #[test]
    fn test_subagent_event_roundtrip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let event = AppEvent::SubagentFinished {
            subagent_name: "explorer".to_string(),
            success: true,
            result_summary: "Found 3 files".to_string(),
            tool_call_count: 5,
            shallow_warning: None,
        };

        {
            let mut recorder = EventRecorder::new(&path).unwrap();
            recorder.record(&event);
        }

        let events = load_recorded_events(&path).unwrap();
        assert_eq!(events.len(), 1);
        let reconstructed = events[0].to_app_event().unwrap();
        match reconstructed {
            AppEvent::SubagentFinished {
                subagent_name,
                success,
                result_summary,
                tool_call_count,
                shallow_warning,
            } => {
                assert_eq!(subagent_name, "explorer");
                assert!(success);
                assert_eq!(result_summary, "Found 3 files");
                assert_eq!(tool_call_count, 5);
                assert!(shallow_warning.is_none());
            }
            _ => panic!("Wrong event variant"),
        }
    }

    #[test]
    fn test_terminal_events_not_reconstructed() {
        let recorded = RecordedEvent {
            seq: 1,
            timestamp_ms: 0,
            variant: "Terminal".to_string(),
            payload: serde_json::json!("some debug string"),
        };
        assert!(recorded.to_app_event().is_none());
    }
}
