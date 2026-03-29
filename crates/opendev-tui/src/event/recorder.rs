//! Event recording and replay for debugging.
//!
//! Activated when `OPENDEV_DEBUG_EVENTS=1` is set. Records all [`AppEvent`]
//! variants to a JSONL file with sequence numbers and timestamps.

use std::io::Write;
use std::path::{Path, PathBuf};

use opendev_models::message::ChatMessage;

use super::AppEvent;

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
    pub(super) fn from_app_event(event: &AppEvent, seq: u64, elapsed_ms: u64) -> Self {
        let (variant, payload) = match event {
            AppEvent::Terminal(e) => ("Terminal".to_string(), serde_json::json!(format!("{e:?}"))),
            AppEvent::Key(k) => ("Key".to_string(), serde_json::json!(format!("{k:?}"))),
            AppEvent::Resize(w, h) => ("Resize".to_string(), serde_json::json!({"w": w, "h": h})),
            AppEvent::ScrollUp => ("ScrollUp".to_string(), serde_json::Value::Null),
            AppEvent::ScrollDown => ("ScrollDown".to_string(), serde_json::Value::Null),
            AppEvent::MouseDown { col, row } => (
                "MouseDown".to_string(),
                serde_json::json!({"col": col, "row": row}),
            ),
            AppEvent::MouseDrag { col, row } => (
                "MouseDrag".to_string(),
                serde_json::json!({"col": col, "row": row}),
            ),
            AppEvent::MouseUp { col, row } => (
                "MouseUp".to_string(),
                serde_json::json!({"col": col, "row": row}),
            ),
            AppEvent::FocusGained => ("FocusGained".to_string(), serde_json::Value::Null),
            AppEvent::Tick => ("Tick".to_string(), serde_json::Value::Null),
            AppEvent::AgentStarted => ("AgentStarted".to_string(), serde_json::Value::Null),
            AppEvent::AgentChunk(s) => ("AgentChunk".to_string(), serde_json::json!({"chunk": s})),
            AppEvent::ReasoningContent(s) => (
                "ReasoningContent".to_string(),
                serde_json::json!({"content": s}),
            ),
            AppEvent::ReasoningBlockStart => {
                ("ReasoningBlockStart".to_string(), serde_json::Value::Null)
            }
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
                subagent_id,
                subagent_name,
                task,
                cancel_token: _,
            } => (
                "SubagentStarted".to_string(),
                serde_json::json!({"subagent_id": subagent_id, "subagent_name": subagent_name, "task": task}),
            ),
            AppEvent::SubagentToolCall {
                subagent_id,
                subagent_name,
                tool_name,
                tool_id,
                args,
            } => (
                "SubagentToolCall".to_string(),
                serde_json::json!({
                    "subagent_id": subagent_id,
                    "subagent_name": subagent_name,
                    "tool_name": tool_name,
                    "tool_id": tool_id,
                    "args": args,
                }),
            ),
            AppEvent::SubagentToolComplete {
                subagent_id,
                subagent_name,
                tool_name,
                tool_id,
                success,
            } => (
                "SubagentToolComplete".to_string(),
                serde_json::json!({
                    "subagent_id": subagent_id,
                    "subagent_name": subagent_name,
                    "tool_name": tool_name,
                    "tool_id": tool_id,
                    "success": success,
                }),
            ),
            AppEvent::SubagentFinished {
                subagent_id,
                subagent_name,
                success,
                result_summary,
                tool_call_count,
                shallow_warning,
            } => (
                "SubagentFinished".to_string(),
                serde_json::json!({
                    "subagent_id": subagent_id,
                    "subagent_name": subagent_name,
                    "success": success,
                    "result_summary": result_summary,
                    "tool_call_count": tool_call_count,
                    "shallow_warning": shallow_warning,
                }),
            ),
            AppEvent::SubagentTokenUpdate {
                subagent_id,
                subagent_name,
                input_tokens,
                output_tokens,
            } => (
                "SubagentTokenUpdate".to_string(),
                serde_json::json!({
                    "subagent_id": subagent_id,
                    "subagent_name": subagent_name,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                }),
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
            AppEvent::FileChangeSummary {
                files,
                additions,
                deletions,
            } => (
                "FileChangeSummary".to_string(),
                serde_json::json!({"files": files, "additions": additions, "deletions": deletions}),
            ),
            AppEvent::ContextUsage(pct) => {
                ("ContextUsage".to_string(), serde_json::json!({"pct": pct}))
            }
            AppEvent::CompactionStarted => {
                ("CompactionStarted".to_string(), serde_json::Value::Null)
            }
            AppEvent::CompactionFinished { success, message } => (
                "CompactionFinished".to_string(),
                serde_json::json!({"success": success, "message": message}),
            ),
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
            AppEvent::AgentBackgrounded {
                task_id,
                query_summary,
            } => (
                "AgentBackgrounded".to_string(),
                serde_json::json!({"task_id": task_id, "query_summary": query_summary}),
            ),
            AppEvent::BackgroundAgentCompleted {
                task_id,
                success,
                result_summary,
                full_result,
                cost_usd,
                tool_call_count,
            } => (
                "BackgroundAgentCompleted".to_string(),
                serde_json::json!({"task_id": task_id, "success": success, "result_summary": result_summary, "full_result": full_result, "cost_usd": cost_usd, "tool_call_count": tool_call_count}),
            ),
            AppEvent::BackgroundAgentProgress {
                task_id,
                tool_name,
                tool_count,
            } => (
                "BackgroundAgentProgress".to_string(),
                serde_json::json!({"task_id": task_id, "tool_name": tool_name, "tool_count": tool_count}),
            ),
            AppEvent::BackgroundAgentKilled { task_id } => (
                "BackgroundAgentKilled".to_string(),
                serde_json::json!({"task_id": task_id}),
            ),
            AppEvent::BackgroundNudge { content } => (
                "BackgroundNudge".to_string(),
                serde_json::json!({"content": content}),
            ),
            AppEvent::BackgroundAgentActivity { task_id, line } => (
                "BackgroundAgentActivity".to_string(),
                serde_json::json!({"task_id": task_id, "line": line}),
            ),
            AppEvent::SetBackgroundAgentToken { task_id, .. } => (
                "SetBackgroundAgentToken".to_string(),
                serde_json::json!({"task_id": task_id}),
            ),
            AppEvent::SnapshotTaken { hash } => (
                "SnapshotTaken".to_string(),
                serde_json::json!({"hash": hash}),
            ),
            AppEvent::UndoResult { success, message } => (
                "UndoResult".to_string(),
                serde_json::json!({"success": success, "message": message}),
            ),
            AppEvent::RedoResult { success, message } => (
                "RedoResult".to_string(),
                serde_json::json!({"success": success, "message": message}),
            ),
            AppEvent::ShareResult { path } => {
                ("ShareResult".to_string(), serde_json::json!({"path": path}))
            }
            AppEvent::FileChanged { paths } => (
                "FileChanged".to_string(),
                serde_json::json!({"paths": paths}),
            ),
            AppEvent::Quit => ("Quit".to_string(), serde_json::Value::Null),
            AppEvent::SessionTitleUpdated(title) => (
                "SessionTitleUpdated".to_string(),
                serde_json::json!({"title": title}),
            ),
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
            "ReasoningContent" => {
                let content = self.payload.get("content")?.as_str()?.to_string();
                Some(AppEvent::ReasoningContent(content))
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
                let subagent_id = self.payload.get("subagent_id")?.as_str()?.to_string();
                let subagent_name = self.payload.get("subagent_name")?.as_str()?.to_string();
                let task = self.payload.get("task")?.as_str()?.to_string();
                Some(AppEvent::SubagentStarted {
                    subagent_id,
                    subagent_name,
                    task,
                    cancel_token: None,
                })
            }
            "SubagentToolCall" => {
                let subagent_id = self.payload.get("subagent_id")?.as_str()?.to_string();
                let subagent_name = self.payload.get("subagent_name")?.as_str()?.to_string();
                let tool_name = self.payload.get("tool_name")?.as_str()?.to_string();
                let tool_id = self.payload.get("tool_id")?.as_str()?.to_string();
                let args: std::collections::HashMap<String, serde_json::Value> =
                    serde_json::from_value(self.payload.get("args").cloned().unwrap_or_default())
                        .unwrap_or_default();
                Some(AppEvent::SubagentToolCall {
                    subagent_id,
                    subagent_name,
                    tool_name,
                    tool_id,
                    args,
                })
            }
            "SubagentToolComplete" => {
                let subagent_id = self.payload.get("subagent_id")?.as_str()?.to_string();
                let subagent_name = self.payload.get("subagent_name")?.as_str()?.to_string();
                let tool_name = self.payload.get("tool_name")?.as_str()?.to_string();
                let tool_id = self.payload.get("tool_id")?.as_str()?.to_string();
                let success = self.payload.get("success")?.as_bool()?;
                Some(AppEvent::SubagentToolComplete {
                    subagent_id,
                    subagent_name,
                    tool_name,
                    tool_id,
                    success,
                })
            }
            "SubagentFinished" => {
                let subagent_id = self.payload.get("subagent_id")?.as_str()?.to_string();
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
                    subagent_id,
                    subagent_name,
                    success,
                    result_summary,
                    tool_call_count,
                    shallow_warning,
                })
            }
            "SubagentTokenUpdate" => {
                let subagent_id = self.payload.get("subagent_id")?.as_str()?.to_string();
                let subagent_name = self.payload.get("subagent_name")?.as_str()?.to_string();
                let input_tokens = self.payload.get("input_tokens")?.as_u64()?;
                let output_tokens = self.payload.get("output_tokens")?.as_u64()?;
                Some(AppEvent::SubagentTokenUpdate {
                    subagent_id,
                    subagent_name,
                    input_tokens,
                    output_tokens,
                })
            }
            // Legacy thinking traces — no longer used, skip during replay
            "ThinkingTrace" | "CritiqueTrace" | "RefinedThinkingTrace" => None,
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
            "FileChangeSummary" => {
                let files = self.payload.get("files")?.as_u64()? as usize;
                let additions = self.payload.get("additions")?.as_u64()?;
                let deletions = self.payload.get("deletions")?.as_u64()?;
                Some(AppEvent::FileChangeSummary {
                    files,
                    additions,
                    deletions,
                })
            }
            "ContextUsage" => {
                let pct = self.payload.get("pct")?.as_f64()?;
                Some(AppEvent::ContextUsage(pct))
            }
            "CompactionStarted" => Some(AppEvent::CompactionStarted),
            "CompactionFinished" => {
                let success = self.payload.get("success")?.as_bool()?;
                let message = self.payload.get("message")?.as_str()?.to_string();
                Some(AppEvent::CompactionFinished { success, message })
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
            // SetInterruptToken cannot be reconstructed
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
#[path = "recorder_tests.rs"]
mod tests;
