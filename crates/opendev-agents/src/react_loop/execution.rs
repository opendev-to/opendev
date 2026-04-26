//! Main execution loop: run(), run_inner().

use serde_json::Value;
use std::sync::Mutex;
use std::time::Instant;
use tracing::{debug, info, info_span, warn};

use crate::doom_loop::{DoomLoopAction, RecoveryAction};
use crate::llm_calls::LlmCaller;
use crate::prompts::reminders::{
    MessageClass, append_directive, append_nudge, get_reminder, inject_system_message,
};
use crate::traits::{AgentError, AgentResult, TaskMonitor};
use opendev_context::{ArtifactIndex, ContextCompactor};
use opendev_http::adapted_client::AdaptedClient;
use opendev_runtime::{CostTracker, SessionDebugLogger, TodoManager};
use opendev_tools_core::{ToolContext, ToolRegistry};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use super::ReactLoop;
use super::emitter::IterationEmitter;
use super::loop_state::LoopState;
use super::types::TurnResult;

impl ReactLoop {
    #[allow(clippy::too_many_arguments)]
    pub async fn run<M>(
        &self,
        caller: &LlmCaller,
        http_client: &AdaptedClient,
        messages: &mut Vec<Value>,
        tool_schemas: &[Value],
        tool_registry: &Arc<ToolRegistry>,
        tool_context: &ToolContext,
        task_monitor: Option<&M>,
        event_callback: Option<&dyn crate::traits::AgentEventCallback>,
        cost_tracker: Option<&Mutex<CostTracker>>,
        artifact_index: Option<&Mutex<ArtifactIndex>>,
        compactor: Option<&Mutex<ContextCompactor>>,
        todo_manager: Option<&Mutex<TodoManager>>,
        cancel: Option<&CancellationToken>,
        tool_approval_tx: Option<&opendev_runtime::ToolApprovalSender>,
        debug_logger: Option<&SessionDebugLogger>,
    ) -> Result<AgentResult, AgentError>
    where
        M: TaskMonitor + ?Sized,
    {
        let _react_span = info_span!("react_loop");
        let _react_guard = _react_span.enter();
        drop(_react_guard); // Don't hold guard across awaits; span is still active as parent

        // Run the loop body, then reset any stuck todos on exit (interrupt, error, or completion).
        let result = self
            .run_inner(
                caller,
                http_client,
                messages,
                tool_schemas,
                tool_registry,
                tool_context,
                task_monitor,
                event_callback,
                cost_tracker,
                artifact_index,
                compactor,
                todo_manager,
                cancel,
                tool_approval_tx,
                debug_logger,
            )
            .await;

        // Reset any "doing" todos back to "pending" on exit — mirrors Python's
        // _reset_stuck_todos() in the finally block.
        if let Some(mgr) = todo_manager
            && let Ok(mut mgr) = mgr.lock()
        {
            let reset = mgr.reset_stuck_todos();
            if reset > 0 {
                info!(count = reset, "Reset stuck 'doing' todos back to 'pending'");
            }
        }

        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_inner<M>(
        &self,
        caller: &LlmCaller,
        http_client: &AdaptedClient,
        messages: &mut Vec<Value>,
        tool_schemas: &[Value],
        tool_registry: &Arc<ToolRegistry>,
        tool_context: &ToolContext,
        task_monitor: Option<&M>,
        event_callback: Option<&dyn crate::traits::AgentEventCallback>,
        cost_tracker: Option<&Mutex<CostTracker>>,
        artifact_index: Option<&Mutex<ArtifactIndex>>,
        compactor: Option<&Mutex<ContextCompactor>>,
        todo_manager: Option<&Mutex<TodoManager>>,
        cancel: Option<&CancellationToken>,
        tool_approval_tx: Option<&opendev_runtime::ToolApprovalSender>,
        debug_logger: Option<&SessionDebugLogger>,
    ) -> Result<AgentResult, AgentError>
    where
        M: TaskMonitor + ?Sized,
    {
        let mut state = LoopState::new(&tool_context.working_dir);

        // Tool schema deferral: if core tools are marked, only send core +
        // activated tool schemas to the LLM. This mirrors Claude Code's
        // ToolSearch pattern, reducing input tokens from ~13k to ~6k.
        let core_tools = tool_registry.core_tool_names();
        let use_deferral = !core_tools.is_empty();

        loop {
            state.iteration += 1;
            let iter_start = Instant::now();
            let emitter = IterationEmitter::new(event_callback, state.completion_nudge_sent);

            // Tick proactive reminders and fire any that are due
            state.proactive_reminders.tick();
            for (name, class) in state.proactive_reminders.check_and_fire() {
                let content = get_reminder(name, &[]);
                if !content.is_empty() {
                    inject_system_message(messages, &content, class);
                }
            }

            // Run per-turn context collectors (live data: todos, git, plan mode, etc.)
            {
                // Extract last user query for semantic memory selection (clone to avoid borrow conflict)
                let last_user_query: Option<String> = messages
                    .iter()
                    .rev()
                    .find(|m| m.get("role").and_then(|v| v.as_str()) == Some("user"))
                    .and_then(|m| m.get("content").and_then(|v| v.as_str()))
                    .map(String::from);

                // Snapshot recent messages for session memory extraction
                // (clone avoids borrow conflict with the mutable `messages` below)
                let recent_snapshot: Vec<serde_json::Value> = messages
                    .iter()
                    .rev()
                    .take(30)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();

                // Read cumulative input tokens from cost tracker for session memory thresholds
                let cumulative_tokens = cost_tracker
                    .and_then(|ct| ct.lock().ok())
                    .map(|ct| ct.total_input_tokens);

                // Read session ID from tool context shared state
                let session_id_owned: Option<String> = tool_context
                    .shared_state
                    .as_ref()
                    .and_then(|ss| ss.lock().ok())
                    .and_then(|ss| {
                        ss.get("session_id")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    });

                let turn_ctx = crate::attachments::TurnContext {
                    turn_number: state.iteration,
                    working_dir: &tool_context.working_dir,
                    todo_manager,
                    shared_state: tool_context.shared_state.as_ref().map(|arc| arc.as_ref()),
                    last_user_query: last_user_query.as_deref(),
                    cumulative_input_tokens: cumulative_tokens,
                    session_id: session_id_owned.as_deref(),
                    recent_messages: Some(&recent_snapshot),
                };
                state.collector_runner.run(&turn_ctx, messages).await;
            }

            if let Some(result) = super::phases::check_safety(
                self,
                caller,
                http_client,
                messages,
                &mut state,
                task_monitor,
                cost_tracker,
                compactor,
                cancel,
            )
            .await
            {
                return result;
            }

            // Build active tool schemas: core tools + any activated via ToolSearch
            let active_schemas: Vec<Value>;
            let schemas_to_send = if use_deferral {
                active_schemas = tool_schemas
                    .iter()
                    .filter(|s| {
                        let name = s
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        core_tools.contains(name) || state.activated_tools.contains(name)
                    })
                    .cloned()
                    .collect();
                &active_schemas[..]
            } else {
                tool_schemas
            };

            let llm_result = match super::phases::execute_llm_call(
                caller,
                http_client,
                messages,
                schemas_to_send,
                &state,
                &emitter,
                task_monitor,
                cancel,
                debug_logger,
                Some(tool_registry),
                Some(tool_context),
            )
            .await
            {
                Ok(result) => result,
                Err(super::types::LoopAction::Continue) => continue,
                Err(super::types::LoopAction::Return(result)) => return result,
            };
            let super::phases::LlmCallResult {
                body,
                llm_latency_ms,
                streaming_executor,
            } = llm_result;

            let super::phases::ProcessedResponse {
                response,
                turn,
                mut iter_metrics,
            } = super::phases::process_response(
                self,
                caller,
                &body,
                llm_latency_ms,
                messages,
                &mut state,
                &emitter,
                task_monitor,
                cost_tracker,
                compactor,
            )?;

            match turn {
                TurnResult::Interrupted => {
                    iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                    self.push_metrics(iter_metrics);
                    if task_monitor.is_some_and(|m| m.is_background_requested()) {
                        info!(
                            iteration = state.iteration,
                            "Background requested (TurnResult) — yielding"
                        );
                        return Ok(AgentResult::backgrounded(messages.clone()));
                    }
                    return Ok(AgentResult::interrupted(messages.clone()));
                }
                TurnResult::MaxIterations => {
                    iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                    self.push_metrics(iter_metrics);
                    let max = self.config.max_iterations.unwrap_or(0);
                    warn!(
                        iteration = state.iteration,
                        max_iterations = max,
                        "React loop hit max_iterations ceiling — aborting"
                    );
                    let mut result = AgentResult::fail(
                        format!("Reached max_iterations ({max}) without completion"),
                        messages.clone(),
                    );
                    result.completion_status = Some("max_iterations_reached".to_string());
                    return Ok(result);
                }
                TurnResult::Complete { content, status } => {
                    iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                    match super::phases::handle_completion(
                        self,
                        content,
                        status,
                        &response,
                        messages,
                        &mut state,
                        iter_metrics,
                        task_monitor,
                        todo_manager,
                        event_callback,
                    ) {
                        super::types::LoopAction::Continue => continue,
                        super::types::LoopAction::Return(result) => return result,
                    }
                }
                TurnResult::ToolCall { tool_calls } => {
                    // Doom-loop detection with recovery actions
                    let (doom_action, doom_warning) = state.doom_detector.check(&tool_calls);
                    match doom_action {
                        DoomLoopAction::ForceStop => {
                            warn!(
                                nudge_count = state.doom_detector.nudge_count(),
                                "Doom loop force-stop: {}", doom_warning
                            );
                            iter_metrics.total_duration_ms =
                                iter_start.elapsed().as_millis() as u64;
                            self.push_metrics(iter_metrics);
                            return Ok(AgentResult::fail(
                                get_reminder("doom_loop_force_stop_message", &[]),
                                messages.clone(),
                            ));
                        }
                        DoomLoopAction::Redirect | DoomLoopAction::Notify => {
                            inject_system_message(messages, &doom_warning, MessageClass::Internal);
                            let recovery = state.doom_detector.recovery_action(&doom_action);
                            match recovery {
                                RecoveryAction::Nudge(nudge_msg) => {
                                    debug!("Doom loop nudge: {}", nudge_msg);
                                    append_nudge(messages, &nudge_msg);
                                }
                                RecoveryAction::StepBack(step_msg) => {
                                    warn!("Doom loop step-back: {}", step_msg);
                                    append_directive(messages, &step_msg);
                                }
                                RecoveryAction::CompactContext => {
                                    warn!("Doom loop context compaction: {}", doom_warning);
                                    append_directive(
                                        messages,
                                        &get_reminder("doom_loop_compact_directive", &[]),
                                    );
                                }
                            }
                        }
                        DoomLoopAction::None => {}
                    }

                    // Try parallel execution (all spawn_subagent calls)
                    if let Some(action) = super::phases::execute_parallel(
                        self,
                        &tool_calls,
                        messages,
                        &state,
                        &emitter,
                        &mut iter_metrics,
                        iter_start,
                        response.content.as_deref(),
                        tool_registry,
                        tool_context,
                        task_monitor,
                        cancel,
                    )
                    .await
                    {
                        match action {
                            super::types::LoopAction::Continue => continue,
                            super::types::LoopAction::Return(result) => return result,
                        }
                    }

                    // Try batched execution (read-only parallelism)
                    if let Some(action) = super::phases::execute_batched(
                        self,
                        &tool_calls,
                        &response,
                        messages,
                        &mut state,
                        &emitter,
                        &mut iter_metrics,
                        iter_start,
                        tool_registry,
                        tool_context,
                        task_monitor,
                        artifact_index,
                        todo_manager,
                        cancel,
                        tool_approval_tx,
                        streaming_executor.as_ref(),
                    )
                    .await
                    {
                        match action {
                            super::types::LoopAction::Continue => continue,
                            super::types::LoopAction::Return(result) => return result,
                        }
                    }

                    // Sequential tool execution (fallback for single tools / no parallelism)
                    if let Some(action) = super::phases::execute_sequential(
                        self,
                        &tool_calls,
                        &response,
                        messages,
                        &mut state,
                        &emitter,
                        &mut iter_metrics,
                        iter_start,
                        tool_registry,
                        tool_context,
                        task_monitor,
                        artifact_index,
                        todo_manager,
                        cancel,
                        tool_approval_tx,
                        streaming_executor.as_ref(),
                    )
                    .await
                    {
                        match action {
                            super::types::LoopAction::Continue => continue,
                            super::types::LoopAction::Return(result) => return result,
                        }
                    }
                }
                TurnResult::Continue => {
                    // LLM returned failure, loop will retry
                }
            }

            // Finalize metrics for this iteration
            iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
            self.push_metrics(iter_metrics);
        }
    }

    /// Track exploration tool calls for planning phase transitions.
    ///
    /// When `shared_state` has `planning_phase == "explore"`, any exploration
    /// tool call (list_files, read_file, search, grep) increments `explore_count`
    /// and transitions the phase to "plan", then injects a reminder nudging
    /// the LLM to spawn Planner.
    pub(super) fn track_exploration_tools(
        tool_context: &opendev_tools_core::ToolContext,
        tool_names: &[String],
        messages: &mut Vec<Value>,
    ) {
        let shared = match tool_context.shared_state.as_ref() {
            Some(s) => s,
            None => return,
        };
        const EXPLORATION_TOOLS: &[&str] = &[
            "Glob",
            "list_files",
            "Read",
            "read_file",
            "Grep",
            "search",
            "grep",
        ];
        let has_exploration = tool_names
            .iter()
            .any(|name| EXPLORATION_TOOLS.contains(&name.as_str()));
        if !has_exploration {
            return;
        }
        let transitioned = if let Ok(mut state) = shared.lock() {
            let count = state
                .get("explore_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let exploration_count = tool_names
                .iter()
                .filter(|n| EXPLORATION_TOOLS.contains(&n.as_str()))
                .count() as u64;
            state.insert(
                "explore_count".into(),
                serde_json::json!(count + exploration_count),
            );
            if state.get("planning_phase").and_then(|v| v.as_str()) == Some("explore") {
                state.insert("planning_phase".into(), serde_json::json!("plan"));
                // Get plan_file_path for the reminder
                state
                    .get("plan_file_path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        } else {
            None
        };
        // Inject explore_phase_complete reminder after phase transition
        if let Some(plan_file_path) = transitioned {
            let reminder = get_reminder(
                "explore_phase_complete",
                &[("plan_file_path", &plan_file_path)],
            );
            if !reminder.is_empty() {
                append_directive(messages, &reminder);
            }
        }
    }
}
