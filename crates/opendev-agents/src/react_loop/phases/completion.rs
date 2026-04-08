//! TurnResult::Complete handling: truncation, todo nudges, completion nudge.

use std::sync::Mutex;

use serde_json::Value;
use tracing::{info, warn};

use crate::prompts::reminders::{append_directive, append_nudge, get_reminder};
use crate::traits::{AgentEventCallback, AgentResult, LlmResponse, TaskMonitor};
use opendev_runtime::{TodoManager, TodoStatus, play_finish_sound};

use super::super::ReactLoop;
use super::super::loop_state::LoopState;
use super::super::types::{IterationMetrics, LoopAction};

/// Handle the `TurnResult::Complete` branch of the react loop.
///
/// Returns `LoopAction::Continue` when a nudge was injected and the loop
/// should re-iterate, or `LoopAction::Return` with the final result.
#[allow(clippy::too_many_arguments)]
pub(in crate::react_loop) fn handle_completion<M>(
    react_loop: &ReactLoop,
    content: String,
    status: Option<String>,
    response: &LlmResponse,
    messages: &mut Vec<Value>,
    state: &mut LoopState,
    iter_metrics: IterationMetrics,
    task_monitor: Option<&M>,
    todo_manager: Option<&Mutex<TodoManager>>,
    event_callback: Option<&dyn AgentEventCallback>,
) -> LoopAction
where
    M: TaskMonitor + ?Sized,
{
    // Check for output truncation (finish_reason == "length")
    if response.finish_reason.as_deref() == Some("length") && state.consecutive_truncations < 3 {
        state.consecutive_truncations += 1;
        warn!(
            consecutive_truncations = state.consecutive_truncations,
            "Response truncated due to output token limit, continuing"
        );
        append_directive(
            messages,
            &get_reminder("truncation_continue_directive", &[]),
        );
        react_loop.push_metrics(iter_metrics);
        return LoopAction::Continue;
    }
    state.consecutive_truncations = 0;

    // Block completion when there are incomplete todos with work in progress
    if let Some(mgr) = todo_manager
        && let Ok(mgr) = mgr.lock()
        && mgr.has_incomplete_todos()
        && mgr.has_work_in_progress()
        && state.todo_nudge_count < react_loop.config.max_todo_nudges
    {
        state.todo_nudge_count += 1;
        let count = mgr.total() - mgr.completed_count();
        let titles: Vec<_> = mgr
            .all()
            .iter()
            .filter(|t| t.status != TodoStatus::Completed)
            .take(3)
            .map(|t| format!("  - {}", t.title))
            .collect();
        let nudge = get_reminder(
            "incomplete_todos_nudge",
            &[
                ("count", &count.to_string()),
                ("todo_list", &titles.join("\n")),
            ],
        );
        append_nudge(messages, &nudge);
        react_loop.push_metrics(iter_metrics);
        return LoopAction::Continue;
    }

    // Block completion when background tasks are still pending.
    // Count spawned background tasks (from tool_dispatch tracking) vs
    // completed ones (synthetic get_background_result messages in history).
    // This check is repeatable — it keeps blocking until all results arrive.
    if state.bg_tasks_spawned > 0 {
        let bg_completed = messages
            .iter()
            .filter(|m| {
                m.get("name")
                    .and_then(|n| n.as_str())
                    .is_some_and(|n| n == "get_background_result")
            })
            .count();
        if bg_completed < state.bg_tasks_spawned {
            let pending = state.bg_tasks_spawned - bg_completed;
            // Limit nudges to avoid infinite loops (max 10 background wait nudges)
            if state.bg_wait_nudge_count < 10 {
                state.bg_wait_nudge_count += 1;
                info!(
                    spawned = state.bg_tasks_spawned,
                    completed = bg_completed,
                    pending,
                    nudge_count = state.bg_wait_nudge_count,
                    "Blocking completion — background tasks still running"
                );
                let nudge = format!(
                    "You have {pending} background task(s) still running. \
                     Do NOT duplicate their work or call TeamDelete. \
                     Do NOT call get_background_result — results arrive automatically. \
                     Wait for the background completion notifications before finishing."
                );
                append_nudge(messages, &nudge);
                react_loop.push_metrics(iter_metrics);
                return LoopAction::Continue;
            }
        }
    }

    // Implicit completion nudge — verify original task before finishing
    // Skip when no tools were used: pure conversational replies don't need verification
    let has_used_tools = state.iteration > state.consecutive_no_tool_calls;
    if !state.completion_nudge_sent
        && has_used_tools
        && let Some(task) = react_loop.config.original_task.as_deref()
    {
        state.completion_nudge_sent = true;
        info!(
            iteration = state.iteration,
            content_len = content.len(),
            content_preview = opendev_runtime::safe_truncate(&content, 80),
            "Completion nudge firing — pre-nudge content"
        );
        let nudge = get_reminder("implicit_completion_nudge", &[("original_task", task)]);
        append_nudge(messages, &nudge);
        react_loop.push_metrics(iter_metrics);
        return LoopAction::Continue;
    }

    // Check for background request before accepting completion
    if task_monitor.is_some_and(|m| m.is_background_requested()) {
        info!(
            iteration = state.iteration,
            "Background requested at completion — yielding"
        );
        react_loop.push_metrics(iter_metrics);
        return LoopAction::Return(Ok(AgentResult::backgrounded(messages.clone())));
    }

    react_loop.push_metrics(iter_metrics);

    // If content was suppressed during nudge verification, emit it now
    if state.completion_nudge_sent {
        info!(
            iteration = state.iteration,
            content_len = content.len(),
            content_preview = opendev_runtime::safe_truncate(&content, 120),
            "Post-nudge acceptance — emitting suppressed content"
        );
        if !content.is_empty()
            && let Some(cb) = event_callback
        {
            cb.on_agent_chunk(&content);
        }
    }

    // Play completion sound (respects 30s cooldown)
    play_finish_sound();
    let mut result = AgentResult::ok(content, messages.clone());
    result.completion_status = status;
    LoopAction::Return(Ok(result))
}
