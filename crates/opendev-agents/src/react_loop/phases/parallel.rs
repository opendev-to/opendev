//! Parallel subagent execution path.

use std::sync::Arc;

use serde_json::Value;
use tracing::info;

use crate::agent_types::PartialResult;
use crate::traits::{AgentResult, TaskMonitor};
use opendev_tools_core::{ToolContext, ToolRegistry};
use tokio_util::sync::CancellationToken;

use super::super::ReactLoop;
use super::super::emitter::{IterationEmitter, tool_result_display_output};
use super::super::loop_state::LoopState;
use super::super::types::{IterationMetrics, LoopAction};

/// Execute tool calls in parallel when all are `spawn_subagent`.
///
/// Returns `None` if the tool calls are not all `spawn_subagent` (caller should
/// fall through to sequential execution). Returns `Some(LoopAction)` otherwise.
#[allow(clippy::too_many_arguments)]
pub(in crate::react_loop) async fn execute_parallel<M>(
    react_loop: &ReactLoop,
    tool_calls: &[Value],
    messages: &mut Vec<Value>,
    state: &LoopState,
    emitter: &IterationEmitter<'_>,
    iter_metrics: &mut IterationMetrics,
    iter_start: std::time::Instant,
    response_content: Option<&str>,
    tool_registry: &Arc<ToolRegistry>,
    tool_context: &ToolContext,
    task_monitor: Option<&M>,
    cancel: Option<&CancellationToken>,
) -> Option<LoopAction>
where
    M: TaskMonitor + ?Sized,
{
    // Check if all calls are spawn_subagent
    let all_subagents = !tool_calls.is_empty()
        && tool_calls.iter().all(|tc| {
            tc.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .is_some_and(|n| matches!(n, "Agent" | "spawn_subagent"))
        });

    if !all_subagents {
        return None;
    }

    let max_parallel: usize = 25;
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel));

    // Build futures for each tool call
    let futures: Vec<_> = tool_calls
        .iter()
        .map(|tc| {
            let tool_call_id = tc
                .get("id")
                .and_then(|id| id.as_str())
                .unwrap_or("unknown")
                .to_string();
            let tool_name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let args_str = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let args_value: Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
            let args_map: std::collections::HashMap<String, Value> = args_value
                .as_object()
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();

            let wd_str = tool_context.working_dir.to_string_lossy().to_string();
            let args_map = opendev_tools_core::normalizer::normalize_params(
                &tool_name,
                args_map,
                Some(&wd_str),
            );

            emitter.emit_tool_started(&tool_call_id, &tool_name, &args_map);

            let exec_ctx = match cancel {
                Some(ct) => {
                    let mut ctx = tool_context.clone();
                    ctx.cancel_token = Some(ct.child_token());
                    ctx
                }
                None => tool_context.clone(),
            };
            let sem = Arc::clone(&semaphore);

            async move {
                let _permit = sem.acquire().await;
                let result = tool_registry.execute(&tool_name, args_map, &exec_ctx).await;
                (tool_call_id, tool_name, result)
            }
        })
        .collect();

    // Execute all in parallel with cancellation support
    let results = match cancel {
        Some(ct) => {
            tokio::select! {
                results = futures::future::join_all(futures) => results,
                _ = ct.cancelled() => {
                    if task_monitor.is_some_and(|m| m.is_background_requested()) {
                        info!(iteration = state.iteration, "Background requested during parallel tools — yielding");
                        for tc in tool_calls {
                            let tc_id = tc.get("id").and_then(|id| id.as_str()).unwrap_or("unknown");
                            let t_name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("unknown");
                            messages.push(serde_json::json!({
                                "role": "tool",
                                "tool_call_id": tc_id,
                                "name": t_name,
                                "content": "Agent spawned successfully. Running independently in the background.",
                            }));
                        }
                        iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                        react_loop.push_metrics(iter_metrics.clone());
                        return Some(LoopAction::Return(Ok(AgentResult::backgrounded(messages.clone()))));
                    }
                    for tc in tool_calls {
                        let tc_id = tc.get("id").and_then(|id| id.as_str()).unwrap_or("unknown");
                        let t_name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("unknown");
                        emitter.emit_tool_result(tc_id, t_name, "Interrupted by user", false);
                        emitter.emit_tool_finished(tc_id, false);
                        messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tc_id,
                            "name": t_name,
                            "content": "Interrupted by user",
                        }));
                    }
                    iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                    react_loop.push_metrics(iter_metrics.clone());
                    return Some(LoopAction::Return(Ok(AgentResult::interrupted(messages.clone()))));
                }
            }
        }
        None => futures::future::join_all(futures).await,
    };

    let mut _any_tool_failed = false;
    let mut parallel_tool_names: Vec<String> = Vec::new();

    for (tc_id, t_name, tool_result) in results {
        parallel_tool_names.push(t_name.clone());
        {
            let output_str = tool_result_display_output(&tool_result);
            emitter.emit_tool_result(&tc_id, &t_name, &output_str, tool_result.success);
            emitter.emit_tool_finished(&tc_id, tool_result.success);
        }

        if !tool_result.success {
            _any_tool_failed = true;
        }

        let result_value = if tool_result.success {
            serde_json::json!({
                "success": true,
                "output": tool_result.output.as_deref().unwrap_or(""),
            })
        } else {
            serde_json::json!({
                "success": false,
                "error": tool_result.error.as_deref()
                    .unwrap_or("Tool execution failed"),
            })
        };

        let formatted = ReactLoop::format_tool_result(&t_name, &result_value);
        messages.push(serde_json::json!({
            "role": "tool",
            "tool_call_id": tc_id,
            "name": t_name,
            "content": formatted,
        }));
    }

    // Track exploration tools for planning phase transition
    ReactLoop::track_exploration_tools(tool_context, &parallel_tool_names, messages);

    // Check for interrupt after parallel execution
    let interrupted_by_monitor = task_monitor.is_some_and(|m| m.should_interrupt());
    let interrupted_by_cancel = cancel.is_some_and(|c| c.is_cancelled());
    if interrupted_by_monitor || interrupted_by_cancel {
        if task_monitor.is_some_and(|m| m.is_background_requested()) {
            info!(
                iteration = state.iteration,
                "Background requested after parallel tools — yielding"
            );
            iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
            react_loop.push_metrics(iter_metrics.clone());
            return Some(LoopAction::Return(Ok(AgentResult::backgrounded(
                messages.clone(),
            ))));
        }
        let partial = PartialResult::from_interrupted_state(
            messages,
            response_content,
            state.iteration,
            tool_calls.len(),
            tool_calls.len(),
        );
        iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
        react_loop.push_metrics(iter_metrics.clone());
        let mut result = AgentResult::interrupted(messages.clone());
        result.partial_result = Some(partial);
        return Some(LoopAction::Return(Ok(result)));
    }

    // Skip the sequential loop
    iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
    react_loop.push_metrics(iter_metrics.clone());
    Some(LoopAction::Continue)
}
