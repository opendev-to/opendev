//! Sequential tool execution loop: approval, permissions, execution, post-processing.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::Value;
use tracing::{Instrument, debug, info, info_span};

use crate::agent_types::PartialResult;
use crate::prompts::reminders::{append_directive, append_nudge, get_reminder};
use crate::subagents::spec::PermissionAction;
use crate::traits::{AgentResult, LlmResponse, TaskMonitor};
use opendev_context::ArtifactIndex;
use opendev_runtime::{
    TodoManager, TodoStatus, extract_command_prefix, play_finish_sound, summarize_tool_result,
};
use opendev_tools_core::parallel::{ParallelPolicy, ToolCall as ParallelToolCall};
use opendev_tools_core::{ToolContext, ToolRegistry, ToolResult};
use tokio_util::sync::CancellationToken;

use super::super::ReactLoop;
use super::super::compaction::record_artifact;
use super::super::emitter::{IterationEmitter, tool_result_display_output};
use super::super::loop_state::LoopState;
use super::super::streaming_executor::StreamingToolExecutor;
use super::super::types::{IterationMetrics, LoopAction, READ_OPS, ToolCallMetric};

/// Execute tool calls sequentially, handling permissions, approval, and post-processing.
///
/// Returns `Some(LoopAction)` when the loop should exit or continue early,
/// `None` when all tools executed and the loop should proceed to metrics finalization.
#[allow(clippy::too_many_arguments)]
pub(in crate::react_loop) async fn execute_sequential<M>(
    react_loop: &ReactLoop,
    tool_calls: &[Value],
    response: &LlmResponse,
    messages: &mut Vec<Value>,
    state: &mut LoopState,
    emitter: &IterationEmitter<'_>,
    iter_metrics: &mut IterationMetrics,
    iter_start: Instant,
    tool_registry: &Arc<ToolRegistry>,
    tool_context: &ToolContext,
    task_monitor: Option<&M>,
    artifact_index: Option<&Mutex<ArtifactIndex>>,
    todo_manager: Option<&Mutex<TodoManager>>,
    cancel: Option<&CancellationToken>,
    tool_approval_tx: Option<&opendev_runtime::ToolApprovalSender>,
    streaming_executor: Option<&StreamingToolExecutor>,
) -> Option<LoopAction>
where
    M: TaskMonitor + ?Sized,
{
    let total_tool_count = tool_calls.len();
    let mut completed_tool_count: usize = 0;
    let mut any_tool_failed = false;

    for tc in tool_calls {
        // Check for task_complete — block if started todos are incomplete
        if ReactLoop::is_task_complete(tc) {
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
                continue;
            }
            let (summary, status) = ReactLoop::extract_task_complete_args(tc);
            let display_text = response
                .content
                .as_deref()
                .filter(|c| !c.trim().is_empty())
                .map(|c| c.to_string())
                .unwrap_or(summary);
            emitter.emit_text_if_not_streamed(&display_text);
            iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
            react_loop.push_metrics(iter_metrics.clone());
            play_finish_sound();
            let mut result = AgentResult::ok(display_text, messages.clone());
            result.completion_status = Some(status);
            return Some(LoopAction::Return(Ok(result)));
        }

        let tool_name = tc
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");

        // Use pre-parsed arguments from the streaming executor when available,
        // skipping redundant JSON parsing and normalization.
        let (args_value, mut args_map) = if let Some(executor) = streaming_executor
            && let Some(preparsed) =
                executor.take_preparsed_args(tc.get("id").and_then(|id| id.as_str()).unwrap_or(""))
        {
            debug!(
                tool = tool_name,
                "Using pre-parsed args from streaming executor"
            );
            // Reconstruct args_value from pre-parsed map for record_artifact()
            let value = Value::Object(
                preparsed
                    .args_map
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
            (value, preparsed.args_map)
        } else {
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
                tool_name,
                args_map,
                Some(&wd_str),
            );
            (args_value, args_map)
        };

        let tool_call_id_str = tc.get("id").and_then(|id| id.as_str()).unwrap_or("unknown");

        emitter.emit_tool_started(tool_call_id_str, tool_name, &args_map);

        // Permission enforcement
        let mut permission_allows = false;
        if !react_loop.config.permission.is_empty() {
            let arg_pattern = args_map
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(action) = react_loop
                .config
                .evaluate_permission(tool_name, arg_pattern)
            {
                match action {
                    PermissionAction::Deny => {
                        debug!(tool = tool_name, "Tool call denied by permission rules");
                        let result_content = ReactLoop::format_tool_result(
                            tool_name,
                            &serde_json::json!({
                                "success": false,
                                "error": format!(
                                    "Permission denied: '{}' is not allowed by agent permission rules",
                                    tool_name
                                )
                            }),
                        );
                        messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id_str,
                            "name": tool_name,
                            "content": result_content,
                        }));
                        emitter.emit_tool_result(
                            tool_call_id_str,
                            tool_name,
                            "Permission denied by agent rules",
                            false,
                        );
                        emitter.emit_tool_finished(tool_call_id_str, false);
                        continue;
                    }
                    PermissionAction::Allow => {
                        permission_allows = true;
                    }
                    PermissionAction::Ask => {
                        if !matches!(tool_name, "Bash" | "run_command")
                            && let Some(approval_tx) = tool_approval_tx
                        {
                            let desc = format!("{} {}", tool_name, arg_pattern);
                            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
                            let req = opendev_runtime::ToolApprovalRequest {
                                tool_name: tool_name.to_string(),
                                command: desc,
                                working_dir: tool_context.working_dir.display().to_string(),
                                response_tx: resp_tx,
                            };
                            if approval_tx.send(req).is_ok() {
                                match resp_rx.await {
                                    Ok(d) if !d.approved => {
                                        let result_content = ReactLoop::format_tool_result(
                                            tool_name,
                                            &serde_json::json!({
                                                "success": false,
                                                "error": "Tool call denied by user"
                                            }),
                                        );
                                        messages.push(serde_json::json!({
                                            "role": "tool",
                                            "tool_call_id": tool_call_id_str,
                                            "name": tool_name,
                                            "content": result_content,
                                        }));
                                        emitter.emit_tool_result(
                                            tool_call_id_str,
                                            tool_name,
                                            "Tool call denied by user",
                                            false,
                                        );
                                        emitter.emit_tool_finished(tool_call_id_str, false);
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        // Tool approval gate for bash/run_command and MCP tools
        let needs_approval_gate =
            matches!(tool_name, "Bash" | "run_command") || tool_name.starts_with("mcp__");
        let auto_approved = if matches!(tool_name, "Bash" | "run_command") {
            let cmd = args_map
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            state.auto_approved_patterns.iter().any(|pattern| {
                let cmd_lower = cmd.to_lowercase();
                let pat_lower = pattern.to_lowercase();
                cmd_lower == pat_lower || cmd_lower.starts_with(&format!("{pat_lower} "))
            })
        } else {
            state.auto_approved_patterns.contains(tool_name)
        };
        if needs_approval_gate
            && !permission_allows
            && !auto_approved
            && let Some(approval_tx) = tool_approval_tx
        {
            let command = if matches!(tool_name, "Bash" | "run_command") {
                args_map
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                serde_json::to_string_pretty(&serde_json::Value::Object(
                    args_map
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ))
                .unwrap_or_default()
            };
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
            let req = opendev_runtime::ToolApprovalRequest {
                tool_name: tool_name.to_string(),
                command: command.clone(),
                working_dir: tool_context.working_dir.display().to_string(),
                response_tx: resp_tx,
            };
            if approval_tx.send(req).is_ok() {
                match resp_rx.await {
                    Ok(d) if !d.approved => {
                        let result_content = ReactLoop::format_tool_result(
                            tool_name,
                            &serde_json::json!({"success": false, "error": "Command denied by user"}),
                        );
                        messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id_str,
                            "name": tool_name,
                            "content": result_content,
                        }));
                        emitter.emit_tool_result(
                            tool_call_id_str,
                            tool_name,
                            "Command denied by user",
                            false,
                        );
                        emitter.emit_tool_finished(tool_call_id_str, false);
                        continue;
                    }
                    Ok(d) => {
                        if d.choice == "yes_remember" {
                            if matches!(tool_name, "Bash" | "run_command") {
                                let prefix = extract_command_prefix(d.command.trim());
                                debug!(
                                    prefix = %prefix,
                                    "Auto-approving command prefix for remainder of session"
                                );
                                state.auto_approved_patterns.insert(prefix);
                            } else {
                                state.auto_approved_patterns.insert(tool_name.to_string());
                                debug!(
                                    tool = tool_name,
                                    "Auto-approving tool for remainder of session"
                                );
                            }
                        }
                        if d.command != command {
                            args_map.insert("command".to_string(), serde_json::json!(d.command));
                        }
                    }
                    Err(_) => {}
                }
            }
        }

        // Plan edit review gate — when user selected "review edits" at plan approval,
        // file-writing tools require per-call user approval.
        let is_file_edit_tool = matches!(
            tool_name,
            "Write" | "write_file" | "Edit" | "edit_file" | "multi_edit"
        );
        if is_file_edit_tool
            && state.plan_edit_review_mode
            && !permission_allows
            && let Some(approval_tx) = tool_approval_tx
        {
            let file_path = extract_file_tool_path(tool_name, &args_map)
                .unwrap_or_else(|| "unknown".to_string());
            let preview = format_edit_preview(tool_name, &args_map);
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
            let req = opendev_runtime::ToolApprovalRequest {
                tool_name: format!("{tool_name} (plan review)"),
                command: format!("{file_path}\n{preview}"),
                working_dir: tool_context.working_dir.display().to_string(),
                response_tx: resp_tx,
            };
            if approval_tx.send(req).is_ok() {
                match resp_rx.await {
                    Ok(d) if !d.approved => {
                        let result_content = ReactLoop::format_tool_result(
                            tool_name,
                            &serde_json::json!({
                                "success": false,
                                "error": "File edit denied by user during plan review"
                            }),
                        );
                        messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id_str,
                            "name": tool_name,
                            "content": result_content,
                        }));
                        emitter.emit_tool_result(
                            tool_call_id_str,
                            tool_name,
                            "File edit denied by user during plan review",
                            false,
                        );
                        emitter.emit_tool_finished(tool_call_id_str, false);
                        continue;
                    }
                    Ok(d) if d.choice == "yes_remember" => {
                        // User opted to stop reviewing — switch to auto mode
                        state.plan_edit_review_mode = false;
                    }
                    _ => {}
                }
            }
        }

        // Check for an early result from the streaming executor (read-only tools
        // may have already completed during LLM streaming).
        if let Some(executor) = streaming_executor
            && let Some(early) = executor.take_result(tool_call_id_str)
        {
            debug!(
                tool = tool_name,
                call_id = tool_call_id_str,
                duration_ms = early.duration_ms,
                "Using early result from streaming executor"
            );
            let tool_result = early.result;
            let tool_duration_ms = early.duration_ms;

            iter_metrics.tool_calls.push(ToolCallMetric {
                tool_name: tool_name.to_string(),
                duration_ms: tool_duration_ms,
                success: tool_result.success,
            });

            if tool_result.success
                && let Some(ai) = artifact_index
            {
                record_artifact(ai, tool_name, &args_value, &tool_result);
            }

            let output_str = tool_result_display_output(&tool_result);
            emitter.emit_tool_result(
                tool_call_id_str,
                tool_name,
                &output_str,
                tool_result.success,
            );
            emitter.emit_tool_finished(tool_call_id_str, tool_result.success);

            if !tool_result.success {
                any_tool_failed = true;
            }

            let mut result_value = if tool_result.success {
                serde_json::json!({
                    "success": true,
                    "output": tool_result.output.as_deref().unwrap_or(""),
                })
            } else {
                serde_json::json!({
                    "success": false,
                    "error": tool_result.error.as_deref().unwrap_or("Tool execution failed"),
                })
            };
            if let Some(ref suffix) = tool_result.llm_suffix {
                result_value["llm_suffix"] = serde_json::json!(suffix);
            }

            let formatted = ReactLoop::format_tool_result(tool_name, &result_value);
            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tool_call_id_str,
                "name": tool_name,
                "content": formatted,
            }));

            completed_tool_count += 1;
            continue;
        }

        // Execute the tool
        let exec_tool_context = match cancel {
            Some(ct) => {
                let mut ctx = tool_context.clone();
                ctx.cancel_token = Some(ct.child_token());
                ctx
            }
            None => tool_context.clone(),
        };

        let tool_start = Instant::now();
        let is_task_stop = matches!(tool_name, "TaskStop" | "task_complete");
        let (tool_result, was_interrupted) = {
            let exec_fut = async {
                tool_registry
                    .execute(tool_name, args_map, &exec_tool_context)
                    .await
            }
            .instrument(info_span!(
                "tool_execution",
                tool_name = tool_name,
                tool_call_id = tool_call_id_str,
                iteration = state.iteration,
            ));

            // TaskStop must never be interrupted by background completion events.
            // Cancelling TaskStop creates an orphaned tool call with no result,
            // which triggers repeated nudge → TaskStop → interrupt cycles.
            match cancel {
                Some(ct) if !is_task_stop => {
                    tokio::select! {
                        result = exec_fut => (result, false),
                        _ = ct.cancelled() => {
                            (ToolResult::fail("Interrupted by user"), true)
                        }
                    }
                }
                _ => (exec_fut.await, false),
            }
        };
        let tool_duration_ms = tool_start.elapsed().as_millis() as u64;

        iter_metrics.tool_calls.push(ToolCallMetric {
            tool_name: tool_name.to_string(),
            duration_ms: tool_duration_ms,
            success: tool_result.success,
        });

        // Record file operations in artifact index
        if tool_result.success
            && let Some(ai) = artifact_index
        {
            record_artifact(ai, tool_name, &args_value, &tool_result);
        }

        // Emit tool result (skip if interrupted)
        if !was_interrupted {
            let output_str = tool_result_display_output(&tool_result);
            emitter.emit_tool_result(
                tool_call_id_str,
                tool_name,
                &output_str,
                tool_result.success,
            );
        } else if task_monitor.is_some_and(|m| m.is_background_requested()) {
            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tool_call_id_str,
                "name": tool_name,
                "content": "Agent spawned successfully. Running independently in the background.",
            }));
            iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
            react_loop.push_metrics(iter_metrics.clone());
            return Some(LoopAction::Return(Ok(AgentResult::backgrounded(
                messages.clone(),
            ))));
        }
        emitter.emit_tool_finished(tool_call_id_str, tool_result.success);

        // Result summary for logging
        let _result_summary = summarize_tool_result(
            tool_name,
            tool_result.output.as_deref(),
            if tool_result.success {
                None
            } else {
                tool_result.error.as_deref()
            },
        );
        debug!(tool = tool_name, summary = %_result_summary, "Tool result summary");

        // Format and append tool result to messages
        let mut result_value = if tool_result.success {
            serde_json::json!({
                "success": true,
                "output": tool_result.output.as_deref().unwrap_or(""),
            })
        } else {
            serde_json::json!({
                "success": false,
                "error": tool_result.error.as_deref().unwrap_or("Tool execution failed"),
            })
        };
        if let Some(ref suffix) = tool_result.llm_suffix {
            result_value["llm_suffix"] = serde_json::json!(suffix);
        }

        let formatted = ReactLoop::format_tool_result(tool_name, &result_value);
        let budgeted = opendev_context::apply_tool_result_budget(
            tool_name,
            tool_call_id_str,
            &formatted,
            &state.tool_budget_policy,
            &state.overflow_store,
        );
        if budgeted.truncated {
            debug!(
                tool = tool_name,
                original_len = budgeted.original_len,
                overflow_ref = ?budgeted.overflow_ref,
                "Tool result exceeded budget; truncated with overflow ref",
            );
        }
        messages.push(serde_json::json!({
            "role": "tool",
            "tool_call_id": tool_call_id_str,
            "name": tool_name,
            "content": budgeted.displayed_content,
        }));

        // Track background task spawns for completion nudge.
        // SpawnTeammate always runs in background; SpawnSubagent does when
        // run_in_background=true. Both return "task_id:" on success.
        if tool_result.success
            && matches!(tool_name, "SpawnTeammate" | "Agent" | "spawn_subagent")
            && tool_result
                .output
                .as_deref()
                .is_some_and(|o| o.contains("task_id:") || o.contains("Running in background"))
        {
            state.bg_tasks_spawned += 1;
        }

        // Capture skill model override from invoke_skill
        if matches!(tool_name, "Skill" | "invoke_skill")
            && tool_result.success
            && let Some(model) = tool_result
                .metadata
                .get("skill_model")
                .and_then(|v| v.as_str())
        {
            info!(model, "Skill model override activated");
            state.skill_model_override = Some(model.to_string());
        }

        // Activate deferred tools returned by ToolSearch
        if tool_name == "ToolSearch"
            && tool_result.success
            && let Some(tools) = tool_result
                .metadata
                .get("activated_tools")
                .and_then(|v| v.as_array())
        {
            for name in tools.iter().filter_map(|v| v.as_str()) {
                state.activated_tools.insert(name.to_string());
                info!(tool = %name, "Deferred tool activated via ToolSearch");
            }
        }

        // Reset proactive reminder counters on relevant tool use
        if tool_result.success {
            // Any successful tool resets the general task reminder
            state.proactive_reminders.reset("task_proactive_reminder");
            // Todo tools specifically reset the todo reminder
            if matches!(
                tool_name,
                "TodoWrite"
                    | "write_todos"
                    | "TaskUpdate"
                    | "update_todo"
                    | "complete_todo"
                    | "TaskList"
                    | "list_todos"
            ) {
                state.proactive_reminders.reset("todo_proactive_reminder");
            }
        }

        // Lazy per-subdirectory instruction injection
        if tool_result.success
            && matches!(
                tool_name,
                "Read"
                    | "read_file"
                    | "Edit"
                    | "edit_file"
                    | "Write"
                    | "write_file"
                    | "Grep"
                    | "grep"
            )
        {
            let file_path_str = args_value
                .get("file_path")
                .or_else(|| args_value.get("path"))
                .and_then(|v| v.as_str());
            if let Some(fp) = file_path_str {
                let path = std::path::Path::new(fp);
                let instructions = state.subdir_tracker.check_file_read(path);
                for instr in &instructions {
                    let note = format!(
                        "The following project instructions apply to files in this directory ({}):\n\n{}",
                        instr.relative_path, instr.content,
                    );
                    append_directive(messages, &note);
                    debug!(
                        path = %instr.relative_path,
                        "Injected subdirectory instruction file"
                    );
                }
            }
        }

        // Error directive after tool failure
        if !tool_result.success {
            any_tool_failed = true;
            let error_text = tool_result.error.as_deref().unwrap_or("");
            let error_type = ReactLoop::classify_error(error_text);
            let nudge_name = format!("nudge_{error_type}");
            let nudge = get_reminder(&nudge_name, &[]);
            if nudge.is_empty() {
                let generic = get_reminder("failed_tool_nudge", &[]);
                if !generic.is_empty() {
                    append_directive(messages, &generic);
                }
            } else {
                append_directive(messages, &nudge);
            }
        }

        // Inject plan_approved_signal after successful present_plan / EnterPlanMode
        if matches!(tool_name, "EnterPlanMode" | "present_plan") && tool_result.success {
            let plan_content = tool_result
                .metadata
                .get("plan_content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let reminder = get_reminder("plan_approved_signal", &[("plan_content", plan_content)]);
            if !reminder.is_empty() {
                append_directive(messages, &reminder);
            }

            // Store plan edit review mode from approval decision.
            // When auto_approve is false, user selected "review edits" mode.
            if let Some(auto_approve) = tool_result
                .metadata
                .get("auto_approve")
                .and_then(|v| v.as_bool())
            {
                state.plan_edit_review_mode = !auto_approve;
            }
        }

        completed_tool_count += 1;

        // Track exploration tools for planning phase transition
        ReactLoop::track_exploration_tools(tool_context, &[tool_name.to_string()], messages);

        // Check for interrupt between tool executions
        let interrupted_by_monitor = task_monitor.is_some_and(|m| m.should_interrupt());
        let interrupted_by_cancel = cancel.is_some_and(|c| c.is_cancelled());
        if interrupted_by_monitor || interrupted_by_cancel {
            if task_monitor.is_some_and(|m| m.is_background_requested()) {
                info!(
                    iteration = state.iteration,
                    "Background requested during sequential tools — yielding"
                );
                iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                react_loop.push_metrics(iter_metrics.clone());
                return Some(LoopAction::Return(Ok(AgentResult::backgrounded(
                    messages.clone(),
                ))));
            }

            // Append stub results for remaining unexecuted tool calls
            for remaining_tc in &tool_calls[completed_tool_count..] {
                let tc_id = remaining_tc
                    .get("id")
                    .and_then(|id| id.as_str())
                    .unwrap_or("");
                let tc_name = remaining_tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown");
                messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tc_id,
                    "name": tc_name,
                    "content": "Error: Interrupted by user",
                }));
            }

            let partial_content = response.content.as_deref().unwrap_or("").to_string();
            iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
            react_loop.push_metrics(iter_metrics.clone());

            let partial = PartialResult::from_interrupted_state(
                messages,
                response.content.as_deref(),
                state.iteration,
                completed_tool_count,
                total_tool_count,
            );

            let mut result = AgentResult::interrupted(messages.clone());
            result.partial_result = Some(partial);
            if !partial_content.is_empty() {
                result.content = format!("Task interrupted by user (partial): {}", partial_content);
            }
            return Some(LoopAction::Return(Ok(result)));
        }

        // Check for background request between sequential tool executions
        if task_monitor.is_some_and(|m| m.is_background_requested()) {
            info!(
                iteration = state.iteration,
                "Background requested after sequential tool — yielding"
            );
            iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
            react_loop.push_metrics(iter_metrics.clone());
            return Some(LoopAction::Return(Ok(AgentResult::backgrounded(
                messages.clone(),
            ))));
        }
    }

    // --- Post-tool analysis ---

    // Consecutive reads detection
    let all_reads = tool_calls.iter().all(|tc| {
        let name = tc
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("");
        READ_OPS.contains(&name)
    });
    if all_reads && !any_tool_failed {
        state.consecutive_reads += 1;
        if state.consecutive_reads >= 5 {
            let nudge = get_reminder("consecutive_reads_nudge", &[]);
            if !nudge.is_empty() {
                append_directive(messages, &nudge);
            }
            state.consecutive_reads = 0;
        }
    } else {
        state.consecutive_reads = 0;
    }

    // All-todos-complete signal
    if !state.all_todos_complete_nudged
        && let Some(mgr) = todo_manager
        && let Ok(mgr) = mgr.lock()
        && mgr.has_todos()
        && !mgr.has_incomplete_todos()
    {
        state.all_todos_complete_nudged = true;
        let nudge = get_reminder("all_todos_complete_nudge", &[]);
        if !nudge.is_empty() {
            append_nudge(messages, &nudge);
        }
    }

    None
}

/// Extract the primary file/directory path from a file tool's arguments.
fn extract_file_tool_path(
    tool_name: &str,
    args: &std::collections::HashMap<String, Value>,
) -> Option<String> {
    match tool_name {
        "Read" | "read_file" | "Edit" | "edit_file" | "Write" | "write_file" | "multi_edit"
        | "insert_symbol" => args
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(String::from),
        "NotebookEdit" | "notebook_edit" => args
            .get("notebook_path")
            .and_then(|v| v.as_str())
            .map(String::from),
        "vlm" => args
            .get("image_path")
            .and_then(|v| v.as_str())
            .map(String::from),
        "web_screenshot" => args
            .get("output_path")
            .and_then(|v| v.as_str())
            .map(String::from),
        "Grep" | "grep" | "ast_grep" | "Glob" | "list_files" => {
            args.get("path").and_then(|v| v.as_str()).map(String::from)
        }
        _ => None,
    }
}

/// Maximum lines shown in the edit approval popup preview.
const PREVIEW_MAX_LINES: usize = 20;

/// Build a short preview string for a file-edit tool call (for the approval popup).
fn format_edit_preview(tool_name: &str, args: &std::collections::HashMap<String, Value>) -> String {
    match tool_name {
        "Write" | "write_file" => {
            truncate_preview(args.get("content").and_then(|v| v.as_str()).unwrap_or(""))
        }
        "Edit" | "edit_file" => {
            let old = args
                .get("old_string")
                .or_else(|| args.get("old_content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let new = args
                .get("new_string")
                .or_else(|| args.get("new_content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!(
                "--- old\n{}\n+++ new\n{}",
                truncate_preview(old),
                truncate_preview(new)
            )
        }
        "multi_edit" => {
            let edits = args.get("edits").and_then(|v| v.as_array());
            match edits {
                Some(arr) => format!("{} edit(s) in file", arr.len()),
                None => String::new(),
            }
        }
        _ => String::new(),
    }
}

/// Truncate text to `PREVIEW_MAX_LINES`, appending a count of remaining lines.
fn truncate_preview(text: &str) -> String {
    let all_lines: Vec<&str> = text.lines().collect();
    if all_lines.len() > PREVIEW_MAX_LINES {
        let shown = all_lines[..PREVIEW_MAX_LINES].join("\n");
        format!(
            "{shown}\n... ({} more lines)",
            all_lines.len() - PREVIEW_MAX_LINES
        )
    } else {
        all_lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Batched execution: parallelize consecutive read-only tool calls
// ---------------------------------------------------------------------------

/// Maximum number of read-only tools to execute concurrently within a batch.
const MAX_CONCURRENT_TOOLS: usize = 10;

/// Extract tool name from a raw tool call Value.
fn tool_name_from(tc: &Value) -> &str {
    tc.get("function")
        .and_then(|f| f.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
}

/// Extract tool call ID from a raw tool call Value.
fn tool_call_id_from(tc: &Value) -> &str {
    tc.get("id").and_then(|id| id.as_str()).unwrap_or("unknown")
}

/// Parse arguments from a raw tool call Value.
fn parse_tool_args(tc: &Value) -> (Value, std::collections::HashMap<String, Value>) {
    let args_str = tc
        .get("function")
        .and_then(|f| f.get("arguments"))
        .and_then(|a| a.as_str())
        .unwrap_or("{}");
    let args_value: Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
    let args_map = args_value
        .as_object()
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    (args_value, args_map)
}

/// Execute tool calls using batched parallelism for consecutive read-only tools.
///
/// Returns `None` if no batches have >1 element (no parallelism benefit, caller
/// should fall through to `execute_sequential`). Returns `Some(LoopAction)` when
/// batched execution handled everything.
#[allow(clippy::too_many_arguments)]
pub(in crate::react_loop) async fn execute_batched<M>(
    react_loop: &ReactLoop,
    tool_calls: &[Value],
    response: &LlmResponse,
    messages: &mut Vec<Value>,
    state: &mut LoopState,
    emitter: &IterationEmitter<'_>,
    iter_metrics: &mut IterationMetrics,
    iter_start: Instant,
    tool_registry: &Arc<ToolRegistry>,
    tool_context: &ToolContext,
    task_monitor: Option<&M>,
    artifact_index: Option<&Mutex<ArtifactIndex>>,
    todo_manager: Option<&Mutex<TodoManager>>,
    cancel: Option<&CancellationToken>,
    tool_approval_tx: Option<&opendev_runtime::ToolApprovalSender>,
    streaming_executor: Option<&StreamingToolExecutor>,
) -> Option<LoopAction>
where
    M: TaskMonitor + ?Sized,
{
    // Build ToolCall structs for the partitioner
    let parallel_calls: Vec<ParallelToolCall> = tool_calls
        .iter()
        .map(|tc| {
            let name = tool_name_from(tc).to_string();
            let (args_value, _) = parse_tool_args(tc);
            ParallelToolCall::new(name, args_value)
        })
        .collect();

    // Look up tool instances for input-dependent concurrency decisions
    let tool_instances: Vec<_> = parallel_calls
        .iter()
        .filter_map(|tc| tool_registry.get(&tc.name))
        .collect();
    let tool_refs: Vec<&dyn opendev_tools_core::BaseTool> =
        tool_instances.iter().map(|t| t.as_ref()).collect();

    let batches = if tool_refs.len() == parallel_calls.len() {
        ParallelPolicy::partition_with_tools(&parallel_calls, &tool_refs)
    } else {
        // Fallback if some tools weren't found in registry
        ParallelPolicy::partition(&parallel_calls)
    };

    // If no batch has >1 element, fall through to sequential execution
    if !ParallelPolicy::has_parallel_batches(&batches) {
        return None;
    }

    info!(
        batch_count = batches.len(),
        "Executing tool calls with batched parallelism"
    );

    let total_tool_count = tool_calls.len();
    let mut completed_tool_count: usize = 0;
    let mut any_tool_failed = false;

    for batch in &batches {
        // Check for interruption between batches
        let interrupted_by_monitor = task_monitor.is_some_and(|m| m.should_interrupt());
        let interrupted_by_cancel = cancel.is_some_and(|c| c.is_cancelled());
        if interrupted_by_monitor || interrupted_by_cancel {
            if task_monitor.is_some_and(|m| m.is_background_requested()) {
                info!(
                    iteration = state.iteration,
                    "Background requested during batched tools — yielding"
                );
                iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                react_loop.push_metrics(iter_metrics.clone());
                return Some(LoopAction::Return(Ok(AgentResult::backgrounded(
                    messages.clone(),
                ))));
            }

            // Stub remaining tool calls
            for remaining_batch in
                &batches[batches.iter().position(|b| std::ptr::eq(b, batch)).unwrap()..]
            {
                for &idx in remaining_batch {
                    let tc = &tool_calls[idx];
                    messages.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id_from(tc),
                        "name": tool_name_from(tc),
                        "content": "Error: Interrupted by user",
                    }));
                }
            }

            let partial = PartialResult::from_interrupted_state(
                messages,
                response.content.as_deref(),
                state.iteration,
                completed_tool_count,
                total_tool_count,
            );
            iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
            react_loop.push_metrics(iter_metrics.clone());
            let mut result = AgentResult::interrupted(messages.clone());
            result.partial_result = Some(partial);
            return Some(LoopAction::Return(Ok(result)));
        }

        if batch.len() == 1 {
            // Single-element batch: run through full sequential pipeline
            let idx = batch[0];
            let tc = &tool_calls[idx];

            // Delegate to execute_sequential for single tools — this handles
            // task_complete, permissions, approval gates, external paths, etc.
            // We call execute_sequential with a single-element slice.
            if let Some(action) = execute_sequential(
                react_loop,
                std::slice::from_ref(tc),
                response,
                messages,
                state,
                emitter,
                iter_metrics,
                iter_start,
                tool_registry,
                tool_context,
                task_monitor,
                artifact_index,
                todo_manager,
                cancel,
                tool_approval_tx,
                streaming_executor,
            )
            .await
            {
                return Some(action);
            }
            completed_tool_count += 1;
        } else {
            // Multi-element batch: all read-only, run in parallel
            if let Some(action) = execute_concurrent_batch(
                react_loop,
                tool_calls,
                batch,
                messages,
                state,
                emitter,
                iter_metrics,
                tool_registry,
                tool_context,
                artifact_index,
                cancel,
                &mut any_tool_failed,
            )
            .await
            {
                return Some(action);
            }
            completed_tool_count += batch.len();
        }
    }

    // --- Post-tool analysis (same as execute_sequential) ---

    // Consecutive reads detection
    let all_reads = tool_calls.iter().all(|tc| {
        let name = tool_name_from(tc);
        READ_OPS.contains(&name)
    });
    if all_reads && !any_tool_failed {
        state.consecutive_reads += 1;
        if state.consecutive_reads >= 5 {
            let nudge = get_reminder("consecutive_reads_nudge", &[]);
            if !nudge.is_empty() {
                append_directive(messages, &nudge);
            }
            state.consecutive_reads = 0;
        }
    } else {
        state.consecutive_reads = 0;
    }

    // All-todos-complete signal
    if !state.all_todos_complete_nudged
        && let Some(mgr) = todo_manager
        && let Ok(mgr) = mgr.lock()
        && mgr.has_todos()
        && !mgr.has_incomplete_todos()
    {
        state.all_todos_complete_nudged = true;
        let nudge = get_reminder("all_todos_complete_nudge", &[]);
        if !nudge.is_empty() {
            append_nudge(messages, &nudge);
        }
    }

    iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
    react_loop.push_metrics(iter_metrics.clone());
    Some(LoopAction::Continue)
}

/// Execute a batch of read-only tools concurrently.
#[allow(clippy::too_many_arguments)]
async fn execute_concurrent_batch(
    _react_loop: &ReactLoop,
    tool_calls: &[Value],
    batch_indices: &[usize],
    messages: &mut Vec<Value>,
    state: &mut LoopState,
    emitter: &IterationEmitter<'_>,
    iter_metrics: &mut IterationMetrics,
    tool_registry: &Arc<ToolRegistry>,
    tool_context: &ToolContext,
    artifact_index: Option<&Mutex<ArtifactIndex>>,
    cancel: Option<&CancellationToken>,
    any_tool_failed: &mut bool,
) -> Option<LoopAction> {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_TOOLS));

    // Pre-process: parse args, normalize, emit tool_started for each tool
    struct ToolPrep {
        #[allow(dead_code)]
        idx: usize,
        tool_call_id: String,
        tool_name: String,
        args_value: Value,
        args_map: std::collections::HashMap<String, Value>,
    }

    let mut preps: Vec<ToolPrep> = Vec::with_capacity(batch_indices.len());
    for &idx in batch_indices {
        let tc = &tool_calls[idx];
        let tool_name = tool_name_from(tc).to_string();
        let tool_call_id = tool_call_id_from(tc).to_string();
        let (args_value, args_map) = parse_tool_args(tc);

        let wd_str = tool_context.working_dir.to_string_lossy().to_string();
        let args_map =
            opendev_tools_core::normalizer::normalize_params(&tool_name, args_map, Some(&wd_str));

        emitter.emit_tool_started(&tool_call_id, &tool_name, &args_map);
        preps.push(ToolPrep {
            idx,
            tool_call_id,
            tool_name,
            args_value,
            args_map,
        });
    }

    if preps.is_empty() {
        return None;
    }

    // Spawn concurrent tool executions
    let futures: Vec<_> = preps
        .iter()
        .map(|prep| {
            let tool_name = prep.tool_name.clone();
            let tool_call_id = prep.tool_call_id.clone();
            let args_map = prep.args_map.clone();
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
                let start = Instant::now();
                let result = tool_registry
                    .execute(&tool_name, args_map, &exec_ctx)
                    .instrument(info_span!(
                        "tool_execution_concurrent",
                        tool_name = %tool_name,
                        tool_call_id = %tool_call_id,
                    ))
                    .await;
                let duration_ms = start.elapsed().as_millis() as u64;
                (tool_call_id, tool_name, result, duration_ms)
            }
        })
        .collect();

    // Await all — read-only tools are safe to complete even on cancellation
    let results = futures::future::join_all(futures).await;

    // Process results in original order (futures::join_all preserves order)
    for (i, (tc_id, t_name, tool_result, duration_ms)) in results.into_iter().enumerate() {
        let prep = &preps[i];

        // Record metric
        iter_metrics.tool_calls.push(ToolCallMetric {
            tool_name: t_name.clone(),
            duration_ms,
            success: tool_result.success,
        });

        // Record artifact
        if tool_result.success
            && let Some(ai) = artifact_index
        {
            record_artifact(ai, &t_name, &prep.args_value, &tool_result);
        }

        // Emit result
        let output_str = tool_result_display_output(&tool_result);
        emitter.emit_tool_result(&tc_id, &t_name, &output_str, tool_result.success);
        emitter.emit_tool_finished(&tc_id, tool_result.success);

        // Result summary for logging
        let _result_summary = summarize_tool_result(
            &t_name,
            tool_result.output.as_deref(),
            if tool_result.success {
                None
            } else {
                tool_result.error.as_deref()
            },
        );
        debug!(tool = %t_name, summary = %_result_summary, "Tool result summary (concurrent)");

        // Format and append tool result to messages
        let mut result_value = if tool_result.success {
            serde_json::json!({
                "success": true,
                "output": tool_result.output.as_deref().unwrap_or(""),
            })
        } else {
            serde_json::json!({
                "success": false,
                "error": tool_result.error.as_deref().unwrap_or("Tool execution failed"),
            })
        };
        if let Some(ref suffix) = tool_result.llm_suffix {
            result_value["llm_suffix"] = serde_json::json!(suffix);
        }

        let formatted = ReactLoop::format_tool_result(&t_name, &result_value);
        messages.push(serde_json::json!({
            "role": "tool",
            "tool_call_id": tc_id,
            "name": t_name,
            "content": formatted,
        }));

        // Reset proactive reminder counters
        if tool_result.success {
            state.proactive_reminders.reset("task_proactive_reminder");
        }

        // Subdirectory instruction injection
        if tool_result.success && matches!(t_name.as_str(), "Read" | "read_file" | "Grep" | "grep")
        {
            let file_path_str = prep
                .args_value
                .get("file_path")
                .or_else(|| prep.args_value.get("path"))
                .and_then(|v| v.as_str());
            if let Some(fp) = file_path_str {
                let path = std::path::Path::new(fp);
                let instructions = state.subdir_tracker.check_file_read(path);
                for instr in &instructions {
                    let note = format!(
                        "The following project instructions apply to files in this directory ({}):\n\n{}",
                        instr.relative_path, instr.content,
                    );
                    append_directive(messages, &note);
                    debug!(
                        path = %instr.relative_path,
                        "Injected subdirectory instruction file (concurrent)"
                    );
                }
            }
        }

        // Error directive after tool failure
        if !tool_result.success {
            *any_tool_failed = true;
            let error_text = tool_result.error.as_deref().unwrap_or("");
            let error_type = ReactLoop::classify_error(error_text);
            let nudge_name = format!("nudge_{error_type}");
            let nudge = get_reminder(&nudge_name, &[]);
            if nudge.is_empty() {
                let generic = get_reminder("failed_tool_nudge", &[]);
                if !generic.is_empty() {
                    append_directive(messages, &generic);
                }
            } else {
                append_directive(messages, &nudge);
            }
        }
    }

    // Track exploration tools
    let tool_names: Vec<String> = preps.iter().map(|p| p.tool_name.clone()).collect();
    ReactLoop::track_exploration_tools(tool_context, &tool_names, messages);

    None
}
