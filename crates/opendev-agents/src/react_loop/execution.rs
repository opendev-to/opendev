//! Main execution loop: run(), run_inner().

use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{Instrument, debug, info, info_span, warn};

use crate::subagents::spec::PermissionAction;

use crate::agent_types::PartialResult;
use crate::doom_loop::{DoomLoopAction, DoomLoopDetector, RecoveryAction};
use crate::llm_calls::LlmCaller;
use crate::prompts::reminders::{
    MessageClass, append_directive, append_nudge, get_reminder, inject_system_message,
};
use crate::traits::{AgentError, AgentResult, TaskMonitor};
use opendev_context::{ArtifactIndex, ContextCompactor};
use opendev_http::adapted_client::AdaptedClient;
use opendev_runtime::{
    CostTracker, TodoManager, TodoStatus, TokenUsage, extract_command_prefix, play_finish_sound,
    summarize_tool_result,
};
use opendev_tools_core::{ToolContext, ToolRegistry, ToolResult};
use tokio_util::sync::CancellationToken;

use super::ReactLoop;
use super::compaction::{apply_staged_compaction, do_llm_compaction, record_artifact};
use super::types::{IterationMetrics, READ_OPS, ToolCallMetric, TurnResult};

/// Per-iteration event emitter that centralizes display-suppression logic.
///
/// During normal iterations, all events pass through to the callback.
/// During completion-nudge verification iterations, text and reasoning
/// events are silently dropped while tool events still pass through.
///
/// This struct is the ONLY way text/reasoning should be emitted to the
/// callback from the react loop. Do NOT call event_callback directly
/// for text/reasoning — always go through the emitter.
struct IterationEmitter<'a> {
    cb: Option<&'a dyn crate::traits::AgentEventCallback>,
    suppress_content: bool,
    text_emitted: AtomicBool,
    reasoning_emitted: AtomicBool,
}

impl<'a> IterationEmitter<'a> {
    fn new(cb: Option<&'a dyn crate::traits::AgentEventCallback>, suppress_content: bool) -> Self {
        Self {
            cb,
            suppress_content,
            text_emitted: AtomicBool::new(false),
            reasoning_emitted: AtomicBool::new(false),
        }
    }

    /// Emit a streaming text chunk. Suppressed during nudge iterations.
    fn emit_text(&self, text: &str) {
        if !self.suppress_content
            && let Some(cb) = self.cb
        {
            cb.on_agent_chunk(text);
            self.text_emitted.store(true, Ordering::Relaxed);
        }
    }

    /// Emit reasoning content. Suppressed during nudge iterations.
    fn emit_reasoning(&self, text: &str) {
        if !self.suppress_content
            && let Some(cb) = self.cb
        {
            cb.on_reasoning(text);
            self.reasoning_emitted.store(true, Ordering::Relaxed);
        }
    }

    /// Emit reasoning block start (separator between interleaved blocks).
    fn emit_reasoning_block_start(&self) {
        if !self.suppress_content
            && let Some(cb) = self.cb
        {
            cb.on_reasoning_block_start();
        }
    }

    /// Emit text if streaming didn't deliver it (non-streaming fallback).
    fn emit_text_if_not_streamed(&self, content: &str) {
        if !self.suppress_content
            && !content.is_empty()
            && !self.text_emitted.load(Ordering::Relaxed)
            && let Some(cb) = self.cb
        {
            cb.on_agent_chunk(content);
            self.text_emitted.store(true, Ordering::Relaxed);
        }
    }

    /// Emit reasoning from response body if streaming didn't already deliver it.
    fn emit_reasoning_if_not_streamed(&self, reasoning: &str) {
        if !self.suppress_content
            && !reasoning.is_empty()
            && !self.reasoning_emitted.load(Ordering::Relaxed)
            && let Some(cb) = self.cb
        {
            cb.on_reasoning(reasoning);
            self.reasoning_emitted.store(true, Ordering::Relaxed);
        }
    }

    // --- Tool events: NEVER suppressed ---

    fn emit_tool_started(
        &self,
        id: &str,
        name: &str,
        args: &std::collections::HashMap<String, serde_json::Value>,
    ) {
        if let Some(cb) = self.cb {
            cb.on_tool_started(id, name, args);
        }
    }

    fn emit_tool_finished(&self, id: &str, success: bool) {
        if let Some(cb) = self.cb {
            cb.on_tool_finished(id, success);
        }
    }

    fn emit_tool_result(&self, id: &str, name: &str, output: &str, success: bool) {
        if let Some(cb) = self.cb {
            cb.on_tool_result(id, name, output, success);
        }
    }

    fn emit_token_usage(&self, input: u64, output: u64) {
        if let Some(cb) = self.cb {
            cb.on_token_usage(input, output);
        }
    }

    fn emit_context_usage(&self, pct: f64) {
        if let Some(cb) = self.cb {
            cb.on_context_usage(pct);
        }
    }
}

impl ReactLoop {
    #[allow(clippy::too_many_arguments)]
    pub async fn run<M>(
        &self,
        caller: &LlmCaller,
        http_client: &AdaptedClient,
        messages: &mut Vec<Value>,
        tool_schemas: &[Value],
        tool_registry: &ToolRegistry,
        tool_context: &ToolContext,
        task_monitor: Option<&M>,
        event_callback: Option<&dyn crate::traits::AgentEventCallback>,
        cost_tracker: Option<&Mutex<CostTracker>>,
        artifact_index: Option<&Mutex<ArtifactIndex>>,
        compactor: Option<&Mutex<ContextCompactor>>,
        todo_manager: Option<&Mutex<TodoManager>>,
        cancel: Option<&CancellationToken>,
        tool_approval_tx: Option<&opendev_runtime::ToolApprovalSender>,
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
        tool_registry: &ToolRegistry,
        tool_context: &ToolContext,
        task_monitor: Option<&M>,
        event_callback: Option<&dyn crate::traits::AgentEventCallback>,
        cost_tracker: Option<&Mutex<CostTracker>>,
        artifact_index: Option<&Mutex<ArtifactIndex>>,
        compactor: Option<&Mutex<ContextCompactor>>,
        todo_manager: Option<&Mutex<TodoManager>>,
        cancel: Option<&CancellationToken>,
        tool_approval_tx: Option<&opendev_runtime::ToolApprovalSender>,
    ) -> Result<AgentResult, AgentError>
    where
        M: TaskMonitor + ?Sized,
    {
        let mut iteration: usize = 0;
        let mut consecutive_no_tool_calls: usize = 0;
        let mut consecutive_truncations: usize = 0;
        let mut doom_detector = DoomLoopDetector::new();

        // Per-subdirectory instruction injection tracker.
        // Initialized with the working dir as root; startup instruction
        // paths are discovered from existing instruction files.
        // startup_paths is kept accessible for reset_after_compaction().
        let startup_paths: Vec<std::path::PathBuf> =
            opendev_context::discover_instruction_files(&tool_context.working_dir)
                .into_iter()
                .map(|f| f.path)
                .collect();
        let mut subdir_tracker = opendev_context::SubdirInstructionTracker::new(
            tool_context.working_dir.clone(),
            &startup_paths,
        );

        // Skill-driven model override: when a skill specifies model: in its
        // frontmatter, use that model for subsequent iterations until reset.
        let mut skill_model_override: Option<String> = None;

        // Session-level auto-approved command prefixes. When a user approves a
        // tool invocation with "yes_remember", the command prefix is added here
        // so future commands with the same prefix skip the approval prompt.
        // For run_command: stores command prefixes (e.g. "cargo test", "npm run").
        // For MCP tools: stores the tool name (e.g. "mcp__slack__send").
        let mut auto_approved_patterns: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Nudge/reminder state tracking
        let mut todo_nudge_count: usize = 0;
        let mut all_todos_complete_nudged = false;
        let mut completion_nudge_sent = false;
        let mut consecutive_reads: usize = 0;

        loop {
            iteration += 1;
            let iter_start = Instant::now();
            let emitter = IterationEmitter::new(event_callback, completion_nudge_sent);

            if self.check_iteration_limit(iteration) {
                info!(
                    iteration,
                    "Max iterations reached — requesting wind-down summary"
                );

                // Inject a prompt asking for a structured summary
                let summary_prompt = get_reminder("safety_limit_summary", &[]);
                append_directive(messages, &summary_prompt);

                // Build payload WITHOUT tools to force text-only response
                let mut payload = caller.build_action_payload(messages, &[]);
                if let Some(obj) = payload.as_object_mut() {
                    obj.remove("tool_choice");
                    obj.remove("tools");
                    // Don't waste tokens on reasoning during wind-down
                    obj.remove("_reasoning_effort");
                }

                match http_client.post_json(&payload, cancel).await {
                    Ok(http_result) if http_result.success => {
                        if let Some(body) = http_result.body {
                            let response = caller.parse_action_response(&body);

                            // Track cost for wind-down call
                            if let Some(ct) = cost_tracker
                                && let Some(ref usage_json) = response.usage
                            {
                                let token_usage = TokenUsage::from_json(usage_json);
                                if let Ok(mut tracker) = ct.lock() {
                                    tracker.record_usage(&token_usage, None);
                                }
                            }

                            if let Some(content) = &response.content {
                                let wind_down_msg = format!(
                                    "[Max iterations ({}) reached — summary below]\n\n{}",
                                    iteration - 1,
                                    content
                                );
                                return Ok(AgentResult::ok(wind_down_msg, messages.clone()));
                            }
                        }
                    }
                    Ok(_) | Err(_) => {
                        warn!("Wind-down LLM call failed, falling back to last content");
                    }
                }

                // Graceful fallback: return last assistant content rather than
                // discarding all work done. The subagent completed iterations
                // of real work — a failed summary call shouldn't negate that.
                let last_content = messages
                    .iter()
                    .rev()
                    .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("Max iterations reached.")
                    .to_string();
                let wind_down_msg = format!(
                    "[Max iterations ({}) reached — summary unavailable]\n\n{}",
                    iteration - 1,
                    last_content
                );
                return Ok(AgentResult::ok(wind_down_msg, messages.clone()));
            }

            // Check for interrupt
            if let Some(monitor) = task_monitor
                && monitor.should_interrupt()
            {
                return Ok(AgentResult::interrupted(messages.clone()));
            }

            // Check for background yield (soft — current iteration already complete)
            if let Some(monitor) = task_monitor
                && monitor.is_background_requested()
            {
                info!(iteration, "Background requested — yielding to foreground");
                return Ok(AgentResult::backgrounded(messages.clone()));
            }

            // Auto-compaction: check context usage and apply staged optimization
            if let Some(comp) = compactor {
                let needs_llm = apply_staged_compaction(comp, messages);
                if needs_llm {
                    do_llm_compaction(comp, messages, caller, http_client).await;
                    // Reset instruction tracker so subdirectory instructions can
                    // be re-injected if compaction removed them from context.
                    subdir_tracker.reset_after_compaction(&startup_paths, messages);
                    info!(
                        injected_remaining = subdir_tracker.injected_count(),
                        "Reset instruction tracker after LLM compaction"
                    );
                }
            }

            // Build payload and send via HttpClient.
            // Apply skill model override if set (from invoke_skill metadata).
            let mut payload = caller.build_action_payload(messages, tool_schemas);
            if let Some(ref override_model) = skill_model_override {
                payload["model"] = serde_json::json!(override_model);
                debug!(iteration, model = %override_model, "Using skill model override");
            }
            debug!(iteration, model = %payload["model"], "ReAct iteration");

            let llm_start = Instant::now();
            let streaming = http_client.supports_streaming();
            debug!(streaming, "LLM call mode");
            let http_result = if streaming {
                // Use SSE streaming — fires callbacks as chunks arrive.
                // Suppression is handled by the emitter automatically.
                let stream_cb = opendev_http::streaming::FnStreamCallback(|event| {
                    use opendev_http::streaming::StreamEvent;
                    match event {
                        StreamEvent::TextDelta(text) => emitter.emit_text(text),
                        StreamEvent::ReasoningDelta(text) => emitter.emit_reasoning(text),
                        StreamEvent::ReasoningBlockStart => emitter.emit_reasoning_block_start(),
                        _ => {}
                    }
                });
                async {
                    http_client
                        .post_json_streaming(&payload, cancel, &stream_cb)
                        .await
                        .map_err(|e| AgentError::LlmError(e.to_string()))
                }
                .instrument(info_span!(
                    "llm_call",
                    iteration = iteration,
                    model = %payload["model"],
                ))
                .await?
            } else {
                async {
                    http_client
                        .post_json(&payload, cancel)
                        .await
                        .map_err(|e| AgentError::LlmError(e.to_string()))
                }
                .instrument(info_span!(
                    "llm_call",
                    iteration = iteration,
                    model = %payload["model"],
                ))
                .await?
            };
            let llm_latency_ms = llm_start.elapsed().as_millis() as u64;

            if http_result.interrupted {
                // Background request also cancels the token — distinguish from hard interrupt
                if task_monitor.is_some_and(|m| m.is_background_requested()) {
                    info!(iteration, "Background requested during LLM call — yielding");
                    return Ok(AgentResult::backgrounded(messages.clone()));
                }
                return Ok(AgentResult::interrupted(messages.clone()));
            }

            if !http_result.success {
                let err_msg = http_result
                    .error
                    .as_deref()
                    .unwrap_or("HTTP request failed");
                warn!(error = err_msg, "LLM HTTP call failed");
                // Transient failure — continue loop (retry on next iteration)
                if http_result.retryable {
                    continue;
                }
                return Err(AgentError::LlmError(err_msg.to_string()));
            }

            let body = http_result
                .body
                .ok_or_else(|| AgentError::LlmError("Empty response body".to_string()))?;

            // Check for API error in response body (e.g. invalid key, bad model)
            if let Some(error_obj) = body.get("error") {
                let msg = error_obj
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown API error");
                return Err(AgentError::LlmError(format!("API error: {msg}")));
            }

            // Parse the response
            let response = caller.parse_action_response(&body);

            // Extract token counts for metrics
            let input_tokens = response
                .usage
                .as_ref()
                .and_then(|u| u.get("prompt_tokens").and_then(|t| t.as_u64()))
                .unwrap_or(0);
            let output_tokens = response
                .usage
                .as_ref()
                .and_then(|u| u.get("completion_tokens").and_then(|t| t.as_u64()))
                .unwrap_or(0);

            // Emit reasoning content to TUI if present.
            // Skip when streaming was used (deltas already delivered).
            // Suppression during nudge iterations is handled by the emitter.
            if let Some(ref reasoning) = response.reasoning_content {
                emitter.emit_reasoning_if_not_streamed(reasoning);
            }

            // Emit text content if streaming didn't deliver it.
            // This covers: non-streaming mode and models/providers that don't
            // send TextDelta events in the stream. Nudge suppression is automatic.
            if let Some(ref content) = response.content {
                emitter.emit_text_if_not_streamed(content);
            }

            // Track token usage
            if let Some(monitor) = task_monitor
                && let Some(ref usage) = response.usage
                && let Some(total) = usage.get("total_tokens").and_then(|t| t.as_u64())
            {
                monitor.update_tokens(total);
            }

            // Emit token usage event to callback
            if input_tokens > 0 || output_tokens > 0 {
                emitter.emit_token_usage(input_tokens, output_tokens);
            }

            // Record cost tracking
            if let Some(ct) = cost_tracker
                && let Some(ref usage_json) = response.usage
            {
                let token_usage = TokenUsage::from_json(usage_json);
                if let Ok(mut tracker) = ct.lock() {
                    tracker.record_usage(&token_usage, None);
                }
            }

            // Calibrate compactor with real API token counts
            if let Some(comp) = compactor
                && input_tokens > 0
                && let Ok(mut c) = comp.lock()
            {
                c.update_from_api_usage(input_tokens, messages.len());
                emitter.emit_context_usage(c.usage_pct());
            }

            // Initialize per-iteration metrics
            let mut iter_metrics = IterationMetrics {
                iteration,
                llm_latency_ms,
                input_tokens,
                output_tokens,
                tool_calls: Vec::new(),
                total_duration_ms: 0,
            };

            // Process the iteration
            let turn = self.process_iteration(
                &response,
                messages,
                iteration,
                &mut consecutive_no_tool_calls,
            )?;

            match turn {
                TurnResult::Interrupted => {
                    iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                    self.push_metrics(iter_metrics);
                    if task_monitor.is_some_and(|m| m.is_background_requested()) {
                        info!(iteration, "Background requested (TurnResult) — yielding");
                        return Ok(AgentResult::backgrounded(messages.clone()));
                    }
                    return Ok(AgentResult::interrupted(messages.clone()));
                }
                TurnResult::MaxIterations => {
                    // This path is reached from process_iteration's secondary
                    // limit check. The primary wind-down happens above, but
                    // this acts as a safety net.
                    iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                    self.push_metrics(iter_metrics);
                    return Ok(AgentResult::fail(
                        "Max iterations reached without completion",
                        messages.clone(),
                    ));
                }
                TurnResult::Complete { content, status } => {
                    // Check for output truncation (finish_reason == "length")
                    if response.finish_reason.as_deref() == Some("length")
                        && consecutive_truncations < 3
                    {
                        consecutive_truncations += 1;
                        warn!(
                            consecutive_truncations,
                            "Response truncated due to output token limit, continuing"
                        );
                        append_directive(
                            messages,
                            &get_reminder("truncation_continue_directive", &[]),
                        );
                        iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                        self.push_metrics(iter_metrics);
                        continue;
                    }
                    consecutive_truncations = 0;

                    // Block completion when there are incomplete todos.
                    if let Some(mgr) = todo_manager
                        && let Ok(mgr) = mgr.lock()
                        && mgr.has_incomplete_todos()
                        && todo_nudge_count < self.config.max_todo_nudges
                    {
                        todo_nudge_count += 1;
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
                        iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                        self.push_metrics(iter_metrics);
                        continue;
                    }

                    // Implicit completion nudge — verify original task before finishing
                    // Skip on first iteration: text-only response = conversational reply
                    if !completion_nudge_sent
                        && iteration > 0
                        && let Some(task) = self.config.original_task.as_deref()
                    {
                        completion_nudge_sent = true;
                        let nudge =
                            get_reminder("implicit_completion_nudge", &[("original_task", task)]);
                        append_nudge(messages, &nudge);
                        iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                        self.push_metrics(iter_metrics);
                        continue;
                    }

                    // Check for background request before accepting completion
                    if task_monitor.is_some_and(|m| m.is_background_requested()) {
                        info!(iteration, "Background requested at completion — yielding");
                        iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                        self.push_metrics(iter_metrics);
                        return Ok(AgentResult::backgrounded(messages.clone()));
                    }

                    iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                    self.push_metrics(iter_metrics);
                    // Play completion sound (respects 30s cooldown)
                    play_finish_sound();
                    let mut result = AgentResult::ok(content, messages.clone());
                    result.completion_status = status;
                    return Ok(result);
                }
                TurnResult::ToolCall { tool_calls } => {
                    // Doom-loop detection with recovery actions
                    let (doom_action, doom_warning) = doom_detector.check(&tool_calls);
                    match doom_action {
                        DoomLoopAction::ForceStop => {
                            warn!(
                                nudge_count = doom_detector.nudge_count(),
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
                            // Log raw diagnostic as Internal (never reaches any model)
                            inject_system_message(messages, &doom_warning, MessageClass::Internal);
                            let recovery = doom_detector.recovery_action(&doom_action);
                            match recovery {
                                RecoveryAction::Nudge(nudge_msg) => {
                                    debug!("Doom loop nudge: {}", nudge_msg);
                                    // Gentle redirect — action model only
                                    append_nudge(messages, &nudge_msg);
                                }
                                RecoveryAction::StepBack(step_msg) => {
                                    warn!("Doom loop step-back: {}", step_msg);
                                    // Strategy change — reaches thinking model too
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

                    // Execute tool calls
                    // Detect if all calls are spawn_subagent — run them in parallel
                    let all_subagents = !tool_calls.is_empty()
                        && tool_calls.iter().all(|tc| {
                            tc.get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                                == Some("spawn_subagent")
                        });

                    if all_subagents {
                        // Parallel subagent execution path using futures::join_all
                        // (no spawning needed — references are valid for the duration)
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
                                let args_value: Value =
                                    serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                                let args_map: std::collections::HashMap<String, Value> = args_value
                                    .as_object()
                                    .map(|obj| {
                                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                                    })
                                    .unwrap_or_default();

                                // Normalize path params for consistent display
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
                                    let result = tool_registry
                                        .execute(&tool_name, args_map, &exec_ctx)
                                        .await;
                                    (tool_call_id, tool_name, result)
                                }
                            })
                            .collect();

                        // Execute all in parallel, with cancellation support
                        let results = match cancel {
                            Some(ct) => {
                                tokio::select! {
                                    results = futures::future::join_all(futures) => results,
                                    _ = ct.cancelled() => {
                                        // Check background FIRST — before emitting any interruption events to TUI
                                        if task_monitor.is_some_and(|m| m.is_background_requested()) {
                                            info!(iteration, "Background requested during parallel tools — yielding");
                                            // Push stub results to keep message history valid, but don't emit to TUI
                                            for tc in &tool_calls {
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
                                            self.push_metrics(iter_metrics);
                                            return Ok(AgentResult::backgrounded(messages.clone()));
                                        }
                                        // Regular interrupt — emit results to TUI
                                        for tc in &tool_calls {
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
                                        self.push_metrics(iter_metrics);
                                        return Ok(AgentResult::interrupted(messages.clone()));
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
                                let output_str = if tool_result.success {
                                    tool_result.output.as_deref().unwrap_or("")
                                } else {
                                    tool_result
                                        .error
                                        .as_deref()
                                        .unwrap_or("Tool execution failed")
                                };
                                emitter.emit_tool_result(
                                    &tc_id,
                                    &t_name,
                                    output_str,
                                    tool_result.success,
                                );
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

                            let formatted = Self::format_tool_result(&t_name, &result_value);
                            messages.push(serde_json::json!({
                                "role": "tool",
                                "tool_call_id": tc_id,
                                "name": t_name,
                                "content": formatted,
                            }));
                        }

                        // Track exploration tools for planning phase transition (parallel)
                        Self::track_exploration_tools(tool_context, &parallel_tool_names, messages);

                        // Check for interrupt after parallel execution
                        let interrupted_by_monitor =
                            task_monitor.is_some_and(|m| m.should_interrupt());
                        let interrupted_by_cancel = cancel.is_some_and(|c| c.is_cancelled());
                        if interrupted_by_monitor || interrupted_by_cancel {
                            // Background request also cancels the token — check before treating as hard interrupt
                            if task_monitor.is_some_and(|m| m.is_background_requested()) {
                                info!(
                                    iteration,
                                    "Background requested after parallel tools — yielding"
                                );
                                iter_metrics.total_duration_ms =
                                    iter_start.elapsed().as_millis() as u64;
                                self.push_metrics(iter_metrics);
                                return Ok(AgentResult::backgrounded(messages.clone()));
                            }
                            let partial = PartialResult::from_interrupted_state(
                                messages,
                                response.content.as_deref(),
                                iteration,
                                tool_calls.len(),
                                tool_calls.len(),
                            );
                            iter_metrics.total_duration_ms =
                                iter_start.elapsed().as_millis() as u64;
                            self.push_metrics(iter_metrics);
                            let mut result = AgentResult::interrupted(messages.clone());
                            result.partial_result = Some(partial);
                            return Ok(result);
                        }

                        // Skip the sequential loop below
                        iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                        self.push_metrics(iter_metrics);
                        continue;
                    }

                    let total_tool_count = tool_calls.len();
                    let mut completed_tool_count: usize = 0;
                    let mut any_tool_failed = false;
                    for tc in &tool_calls {
                        // Check for task_complete — block if todos are incomplete
                        if Self::is_task_complete(tc) {
                            if let Some(mgr) = todo_manager
                                && let Ok(mgr) = mgr.lock()
                                && mgr.has_incomplete_todos()
                                && todo_nudge_count < self.config.max_todo_nudges
                            {
                                todo_nudge_count += 1;
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
                                // Skip task_complete, continue to next tool call
                                // (or continue loop if this was the only call)
                                continue;
                            }
                            let (summary, status) = Self::extract_task_complete_args(tc);
                            // Prefer the assistant's text content over the
                            // task_complete summary.  When thinking guides
                            // the model to produce a natural conversational
                            // reply, the real answer lives in
                            // `response.content` while the summary is just
                            // a terse label like "Greeted the user".
                            let display_text = response
                                .content
                                .as_deref()
                                .filter(|c| !c.trim().is_empty())
                                .map(|c| c.to_string())
                                .unwrap_or(summary);
                            // Emit as agent chunk so TUI displays it — but
                            // skip if streaming already delivered the content
                            // via TextDelta callbacks.
                            emitter.emit_text_if_not_streamed(&display_text);
                            iter_metrics.total_duration_ms =
                                iter_start.elapsed().as_millis() as u64;
                            self.push_metrics(iter_metrics);
                            play_finish_sound();
                            let mut result = AgentResult::ok(display_text, messages.clone());
                            result.completion_status = Some(status);
                            return Ok(result);
                        }

                        let tool_name = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown");

                        let args_str = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");

                        // Parse args JSON string into a HashMap for the registry
                        let args_value: Value =
                            serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                        let args_map: std::collections::HashMap<String, Value> = args_value
                            .as_object()
                            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default();

                        // Normalize path params for consistent display
                        let wd_str = tool_context.working_dir.to_string_lossy().to_string();
                        let mut args_map = opendev_tools_core::normalizer::normalize_params(
                            tool_name,
                            args_map,
                            Some(&wd_str),
                        );

                        let tool_call_id_str =
                            tc.get("id").and_then(|id| id.as_str()).unwrap_or("unknown");

                        emitter.emit_tool_started(tool_call_id_str, tool_name, &args_map);

                        // Per-agent permission enforcement.
                        // For pattern-level rules (e.g. bash commands), use the
                        // command argument as the arg_pattern.
                        let mut permission_allows = false;
                        if !self.config.permission.is_empty() {
                            let arg_pattern = args_map
                                .get("command")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if let Some(action) =
                                self.config.evaluate_permission(tool_name, arg_pattern)
                            {
                                match action {
                                    PermissionAction::Deny => {
                                        debug!(
                                            tool = tool_name,
                                            "Tool call denied by permission rules"
                                        );
                                        let result_content = Self::format_tool_result(
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
                                        // Explicitly allowed — skip the interactive approval
                                        // gate below (even for run_command).
                                        permission_allows = true;
                                    }
                                    PermissionAction::Ask => {
                                        // For non-bash tools, route through the approval
                                        // channel if available.
                                        if tool_name != "run_command"
                                            && let Some(approval_tx) = tool_approval_tx
                                        {
                                            let desc = format!("{} {}", tool_name, arg_pattern);
                                            let (resp_tx, resp_rx) =
                                                tokio::sync::oneshot::channel();
                                            let req = opendev_runtime::ToolApprovalRequest {
                                                tool_name: tool_name.to_string(),
                                                command: desc,
                                                working_dir: tool_context
                                                    .working_dir
                                                    .display()
                                                    .to_string(),
                                                response_tx: resp_tx,
                                            };
                                            if approval_tx.send(req).is_ok() {
                                                match resp_rx.await {
                                                    Ok(d) if !d.approved => {
                                                        let result_content =
                                                            Self::format_tool_result(
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
                                                        emitter.emit_tool_finished(
                                                            tool_call_id_str,
                                                            false,
                                                        );
                                                        continue;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                        // For run_command with Ask, fall through to the
                                        // existing bash approval gate below.
                                    }
                                }
                            }
                        }

                        // Tool approval gate for bash/run_command and MCP tools.
                        // MCP tools (mcp__*) are external and should require approval
                        // by default, same as run_command.
                        // Skip if permission rules explicitly allow this tool,
                        // or if the user previously approved this tool with "always".
                        let needs_approval_gate =
                            tool_name == "run_command" || tool_name.starts_with("mcp__");
                        // Check if this command matches a previously auto-approved prefix.
                        let auto_approved = if tool_name == "run_command" {
                            let cmd = args_map
                                .get("command")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .trim();
                            auto_approved_patterns.iter().any(|pattern| {
                                let cmd_lower = cmd.to_lowercase();
                                let pat_lower = pattern.to_lowercase();
                                cmd_lower == pat_lower
                                    || cmd_lower.starts_with(&format!("{pat_lower} "))
                            })
                        } else {
                            auto_approved_patterns.contains(tool_name)
                        };
                        if needs_approval_gate
                            && !permission_allows
                            && !auto_approved
                            && let Some(approval_tx) = tool_approval_tx
                        {
                            let command = if tool_name == "run_command" {
                                args_map
                                    .get("command")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string()
                            } else {
                                // For MCP and other tools, summarize the args
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
                                        // Push denial as tool result
                                        let result_content = Self::format_tool_result(
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
                                        // "yes_remember" — auto-approve this command prefix for rest of session
                                        if d.choice == "yes_remember" {
                                            if tool_name == "run_command" {
                                                // Extract command prefix (first 1-2 tokens) to allow
                                                // "cargo test --workspace" to match after approving "cargo test".
                                                let prefix =
                                                    extract_command_prefix(d.command.trim());
                                                debug!(
                                                    prefix = %prefix,
                                                    "Auto-approving command prefix for remainder of session"
                                                );
                                                auto_approved_patterns.insert(prefix);
                                            } else {
                                                auto_approved_patterns
                                                    .insert(tool_name.to_string());
                                                debug!(
                                                    tool = tool_name,
                                                    "Auto-approving tool for remainder of session"
                                                );
                                            }
                                        }
                                        // Update command if edited by user
                                        if d.command != command {
                                            args_map.insert(
                                                "command".to_string(),
                                                serde_json::json!(d.command),
                                            );
                                        }
                                    }
                                    Err(_) => {
                                        // Channel dropped — proceed without approval
                                    }
                                }
                            }
                        }

                        // Build tool context with cancel token for this execution
                        let exec_tool_context = match cancel {
                            Some(ct) => {
                                let mut ctx = tool_context.clone();
                                ctx.cancel_token = Some(ct.child_token());
                                ctx
                            }
                            None => tool_context.clone(),
                        };

                        let tool_start = Instant::now();
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
                                iteration = iteration,
                            ));

                            match cancel {
                                Some(ct) => {
                                    tokio::select! {
                                        result = exec_fut => (result, false),
                                        _ = ct.cancelled() => {
                                            (ToolResult::fail("Interrupted by user"), true)
                                        }
                                    }
                                }
                                None => (exec_fut.await, false),
                            }
                        };
                        let tool_duration_ms = tool_start.elapsed().as_millis() as u64;

                        iter_metrics.tool_calls.push(ToolCallMetric {
                            tool_name: tool_name.to_string(),
                            duration_ms: tool_duration_ms,
                            success: tool_result.success,
                        });

                        // Record file operations in the artifact index
                        if tool_result.success
                            && let Some(ai) = artifact_index
                        {
                            record_artifact(ai, tool_name, &args_value, &tool_result);
                        }

                        // Skip emitting the tool result to the TUI when interrupted —
                        // the AgentInterrupted event already shows the message.
                        if !was_interrupted {
                            let output_str = if tool_result.success {
                                tool_result.output.as_deref().unwrap_or("")
                            } else {
                                tool_result
                                    .error
                                    .as_deref()
                                    .unwrap_or("Tool execution failed")
                            };
                            emitter.emit_tool_result(
                                tool_call_id_str,
                                tool_name,
                                output_str,
                                tool_result.success,
                            );
                        } else if task_monitor.is_some_and(|m| m.is_background_requested()) {
                            // Background request — don't emit tool_finished with failure.
                            // Push stub result to messages and return backgrounded immediately.
                            messages.push(serde_json::json!({
                                "role": "tool",
                                "tool_call_id": tool_call_id_str,
                                "name": tool_name,
                                "content": "Agent spawned successfully. Running independently in the background.",
                            }));
                            iter_metrics.total_duration_ms =
                                iter_start.elapsed().as_millis() as u64;
                            self.push_metrics(iter_metrics);
                            return Ok(AgentResult::backgrounded(messages.clone()));
                        }
                        emitter.emit_tool_finished(tool_call_id_str, tool_result.success);

                        // Generate concise summary for session persistence / context
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

                        // Convert ToolResult to the Value format expected by format_tool_result
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

                        let formatted = Self::format_tool_result(tool_name, &result_value);

                        messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id_str,
                            "name": tool_name,
                            "content": formatted,
                        }));

                        // Capture skill model/agent overrides from invoke_skill.
                        // When a skill specifies model: in its frontmatter,
                        // switch to that model for subsequent LLM calls.
                        if tool_name == "invoke_skill"
                            && tool_result.success
                            && let Some(model) = tool_result
                                .metadata
                                .get("skill_model")
                                .and_then(|v| v.as_str())
                        {
                            info!(model, "Skill model override activated");
                            skill_model_override = Some(model.to_string());
                        }

                        // Lazy per-subdirectory instruction injection.
                        // When the agent reads/edits a file, check if there are
                        // AGENTS.md/CLAUDE.md files in that file's directory tree
                        // that haven't been injected yet.
                        if tool_result.success
                            && matches!(
                                tool_name,
                                "read_file" | "edit_file" | "write_file" | "grep"
                            )
                        {
                            let file_path_str = args_value
                                .get("file_path")
                                .or_else(|| args_value.get("path"))
                                .and_then(|v| v.as_str());
                            if let Some(fp) = file_path_str {
                                let path = std::path::Path::new(fp);
                                let instructions = subdir_tracker.check_file_read(path);
                                for instr in &instructions {
                                    let note = format!(
                                        "<system-reminder>\nThe following project instructions apply to files in this directory ({}):\n\n{}\n</system-reminder>",
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

                        // Error directive after tool failure — reaches thinking model
                        // so it can plan a different approach
                        if !tool_result.success {
                            any_tool_failed = true;
                            let error_text = tool_result.error.as_deref().unwrap_or("");
                            let error_type = Self::classify_error(error_text);
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

                        // Inject plan_approved_signal after successful present_plan
                        if tool_name == "present_plan" && tool_result.success {
                            let plan_content = tool_result
                                .metadata
                                .get("plan_content")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let step_count = tool_result
                                .metadata
                                .get("step_count")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let reminder = get_reminder(
                                "plan_approved_signal",
                                &[
                                    ("todos_created", &step_count.to_string()),
                                    ("plan_content", plan_content),
                                ],
                            );
                            if !reminder.is_empty() {
                                append_directive(messages, &reminder);
                            }
                        }

                        completed_tool_count += 1;

                        // Track exploration tools for planning phase transition (sequential)
                        Self::track_exploration_tools(
                            tool_context,
                            &[tool_name.to_string()],
                            messages,
                        );

                        // Check for interrupt between tool executions —
                        // preserve partial work (completed tool results
                        // already appended to messages above).
                        let interrupted_by_monitor =
                            task_monitor.is_some_and(|m| m.should_interrupt());
                        let interrupted_by_cancel = cancel.is_some_and(|c| c.is_cancelled());
                        if interrupted_by_monitor || interrupted_by_cancel {
                            // Background request also cancels the token — check before treating as hard interrupt
                            if task_monitor.is_some_and(|m| m.is_background_requested()) {
                                info!(
                                    iteration,
                                    "Background requested during sequential tools — yielding"
                                );
                                iter_metrics.total_duration_ms =
                                    iter_start.elapsed().as_millis() as u64;
                                self.push_metrics(iter_metrics);
                                return Ok(AgentResult::backgrounded(messages.clone()));
                            }

                            // Append stub results for remaining unexecuted tool calls
                            // so message history doesn't have dangling tool_calls
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
                                    "content": "[Interrupted by user]",
                                }));
                            }

                            // Collect partial assistant text from this iteration
                            let partial_content =
                                response.content.as_deref().unwrap_or("").to_string();

                            iter_metrics.total_duration_ms =
                                iter_start.elapsed().as_millis() as u64;
                            self.push_metrics(iter_metrics);

                            // Build structured partial result
                            let partial = PartialResult::from_interrupted_state(
                                messages,
                                response.content.as_deref(),
                                iteration,
                                completed_tool_count,
                                total_tool_count,
                            );

                            let mut result = AgentResult::interrupted(messages.clone());
                            result.partial_result = Some(partial);
                            if !partial_content.is_empty() {
                                result.content = format!(
                                    "Task interrupted by user (partial): {}",
                                    partial_content
                                );
                            }
                            return Ok(result);
                        }

                        // Check for background request between sequential tool executions
                        if task_monitor.is_some_and(|m| m.is_background_requested()) {
                            info!(
                                iteration,
                                "Background requested after sequential tool — yielding"
                            );
                            iter_metrics.total_duration_ms =
                                iter_start.elapsed().as_millis() as u64;
                            self.push_metrics(iter_metrics);
                            return Ok(AgentResult::backgrounded(messages.clone()));
                        }
                    }

                    // Track todo-creation intent for nudge guard decisions.
                    // If any tool in this batch is write_todos or clear_todos,
                    // mark that we wrote todos and reset the substantive-work flag.
                    // If any tool is NOT a read-only or todo-management tool,
                    // mark that substantive work happened after todo creation.
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
                        consecutive_reads += 1;
                        if consecutive_reads >= 5 {
                            let nudge = get_reminder("consecutive_reads_nudge", &[]);
                            if !nudge.is_empty() {
                                append_directive(messages, &nudge);
                            }
                            consecutive_reads = 0;
                        }
                    } else {
                        consecutive_reads = 0;
                    }

                    // All-todos-complete signal
                    if !all_todos_complete_nudged
                        && let Some(mgr) = todo_manager
                        && let Ok(mgr) = mgr.lock()
                        && mgr.has_todos()
                        && !mgr.has_incomplete_todos()
                    {
                        all_todos_complete_nudged = true;
                        let nudge = get_reminder("all_todos_complete_nudge", &[]);
                        if !nudge.is_empty() {
                            append_nudge(messages, &nudge);
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
    fn track_exploration_tools(
        tool_context: &opendev_tools_core::ToolContext,
        tool_names: &[String],
        messages: &mut Vec<Value>,
    ) {
        let shared = match tool_context.shared_state.as_ref() {
            Some(s) => s,
            None => return,
        };
        const EXPLORATION_TOOLS: &[&str] = &["list_files", "read_file", "search", "grep"];
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
