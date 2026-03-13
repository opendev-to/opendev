//! ReAct loop: reason → decide tool → execute → observe → loop.
//!
//! Mirrors `opendev/core/agents/main_agent/run_loop.py`.
//! The loop iterates up to a configurable maximum, executing tool calls
//! and feeding results back to the LLM until it completes or is interrupted.

use serde_json::Value;
use std::collections::HashSet;
use tracing::{debug, info, warn};

use crate::doom_loop::{DoomLoopAction, DoomLoopDetector};
use crate::llm_calls::LlmCaller;
use crate::response::ResponseCleaner;
use crate::traits::{AgentError, AgentResult, LlmResponse, TaskMonitor};
use opendev_http::adapted_client::AdaptedClient;
use opendev_runtime::ThinkingLevel;
use opendev_tools_core::{ToolContext, ToolRegistry};

/// Tools that are safe for parallel execution (read-only, no side effects).
pub static PARALLELIZABLE_TOOLS: &[&str] = &[
    "read_file",
    "list_files",
    "search",
    "fetch_url",
    "web_search",
    "capture_web_screenshot",
    "analyze_image",
    "list_processes",
    "get_process_output",
    "list_todos",
    "search_tools",
    "find_symbol",
    "find_referencing_symbols",
];

use crate::prompts::embedded;

/// Extended readonly set for thinking-skip heuristic.
/// Matches Python's `IterationMixin._READONLY_TOOLS`.
static READONLY_TOOLS: &[&str] = &[
    "read_file",
    "list_files",
    "search",
    "fetch_url",
    "web_search",
    "find_symbol",
    "find_referencing_symbols",
    "list_todos",
    "search_tools",
    "list_processes",
    "get_process_output",
    "analyze_image",
    "capture_screenshot",
    "capture_web_screenshot",
    "read_pdf",
    "list_sessions",
    "get_session_history",
    "list_subagents",
    "memory_search",
    "list_agents",
];

/// Result of processing a single turn in the ReAct loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnResult {
    /// The loop should continue with the next iteration.
    Continue,
    /// The agent wants to execute tool calls.
    ToolCall {
        /// Tool call objects from the LLM response.
        tool_calls: Vec<Value>,
    },
    /// The agent has completed its task.
    Complete {
        /// Final content from the agent.
        content: String,
        /// Completion status (e.g. "success", "failed").
        status: Option<String>,
    },
    /// Maximum iterations reached.
    MaxIterations,
    /// The run was interrupted by the user.
    Interrupted,
}

/// Configuration for the ReAct loop.
#[derive(Debug, Clone)]
pub struct ReactLoopConfig {
    /// Maximum number of iterations (None = unlimited).
    pub max_iterations: Option<usize>,
    /// Maximum consecutive no-tool-call responses before accepting completion.
    pub max_nudge_attempts: usize,
    /// Maximum todo completion nudges before allowing completion anyway.
    pub max_todo_nudges: usize,
    /// Thinking level — controls whether thinking/critique phases run.
    pub thinking_level: ThinkingLevel,
    /// Pre-composed thinking system prompt (from `create_thinking_composer`).
    /// If `None`, the thinking phase will not swap the system prompt.
    pub thinking_system_prompt: Option<String>,
    /// The user's original task text, used for analysis prompt construction.
    pub original_task: Option<String>,
}

impl Default for ReactLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: None, // Unlimited by default (matches Python)
            max_nudge_attempts: 3,
            max_todo_nudges: 4,
            thinking_level: ThinkingLevel::Medium,
            thinking_system_prompt: None,
            original_task: None,
        }
    }
}

/// The ReAct (Reason-Act) execution loop.
///
/// Orchestrates the cycle of LLM calls and tool executions, handling:
/// - Iteration limits
/// - Interrupt checking
/// - Nudging on failed tools or implicit completion
/// - Parallel execution of read-only tools
/// - Todo completion checking
/// - Doom-loop cycle detection
/// - Thinking-skip heuristic
pub struct ReactLoop {
    config: ReactLoopConfig,
    _cleaner: ResponseCleaner,
    parallelizable: HashSet<&'static str>,
    readonly_tools: HashSet<&'static str>,
}

impl ReactLoop {
    /// Create a new ReAct loop with the given configuration.
    pub fn new(config: ReactLoopConfig) -> Self {
        Self {
            config,
            _cleaner: ResponseCleaner::new(),
            parallelizable: PARALLELIZABLE_TOOLS.iter().copied().collect(),
            readonly_tools: READONLY_TOOLS.iter().copied().collect(),
        }
    }

    /// Update per-query thinking context (original task and system prompt).
    ///
    /// Call this before each `run()` to set the user's original task text
    /// and the pre-composed thinking system prompt.
    pub fn set_thinking_context(
        &mut self,
        original_task: Option<String>,
        thinking_system_prompt: Option<String>,
    ) {
        self.config.original_task = original_task;
        self.config.thinking_system_prompt = thinking_system_prompt;
    }

    /// Create a ReAct loop with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(ReactLoopConfig::default())
    }

    /// Process a single LLM response and determine the next action.
    ///
    /// This is the core decision function of the ReAct loop. It examines
    /// the LLM response and returns a `TurnResult` indicating what should
    /// happen next.
    pub fn process_response(
        &self,
        response: &LlmResponse,
        consecutive_no_tool_calls: usize,
    ) -> TurnResult {
        if response.interrupted {
            return TurnResult::Interrupted;
        }

        if !response.success {
            // Failed API call — if we have an error, treat as needing continuation
            warn!(
                error = response.error.as_deref().unwrap_or("unknown"),
                "LLM call failed"
            );
            return TurnResult::Continue;
        }

        // Check for tool calls
        let tool_calls = response.tool_calls.as_ref().and_then(|tcs| {
            if tcs.is_empty() {
                None
            } else {
                Some(tcs.clone())
            }
        });

        match tool_calls {
            Some(tcs) => TurnResult::ToolCall { tool_calls: tcs },
            None => {
                // No tool calls — check if we should accept completion
                let content = response.content.as_deref().unwrap_or("Done.").to_string();

                if consecutive_no_tool_calls >= self.config.max_nudge_attempts {
                    debug!("Max nudge attempts reached, accepting completion");
                    TurnResult::Complete {
                        content,
                        status: None,
                    }
                } else {
                    // Still have nudge budget — caller decides whether to nudge
                    TurnResult::Complete {
                        content,
                        status: None,
                    }
                }
            }
        }
    }

    /// Check if a set of tool calls are all parallelizable.
    pub fn all_parallelizable(&self, tool_calls: &[Value]) -> bool {
        if tool_calls.len() <= 1 {
            return false;
        }

        tool_calls.iter().all(|tc| {
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            self.parallelizable.contains(name) && name != "task_complete"
        })
    }

    /// Check if a tool call is for task completion.
    pub fn is_task_complete(tool_call: &Value) -> bool {
        tool_call
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str())
            == Some("task_complete")
    }

    /// Extract the summary and status from a task_complete tool call.
    pub fn extract_task_complete_args(tool_call: &Value) -> (String, String) {
        let args_str = tool_call
            .get("function")
            .and_then(|f| f.get("arguments"))
            .and_then(|a| a.as_str())
            .unwrap_or("{}");

        let args: Value = serde_json::from_str(args_str).unwrap_or_default();
        let summary = args
            .get("summary")
            .and_then(|s| s.as_str())
            .unwrap_or("Task completed")
            .to_string();
        let status = args
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("success")
            .to_string();

        (summary, status)
    }

    /// Format a tool execution result into a string for the message history.
    pub fn format_tool_result(tool_name: &str, result: &Value) -> String {
        let success = result
            .get("success")
            .and_then(|s| s.as_bool())
            .unwrap_or(false);

        if success {
            let output = result
                .get("separate_response")
                .or_else(|| result.get("output"))
                .and_then(|o| o.as_str())
                .unwrap_or("");

            let completion_status = result.get("completion_status").and_then(|s| s.as_str());

            if let Some(status) = completion_status {
                format!("[completion_status={status}]\n{output}")
            } else {
                output.to_string()
            }
        } else {
            let error = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Tool execution failed");
            format!("Error in {tool_name}: {error}")
        }
    }

    /// Classify an error for targeted nudge selection.
    pub fn classify_error(error_text: &str) -> &'static str {
        let lower = error_text.to_lowercase();
        if lower.contains("permission denied") {
            "permission_error"
        } else if lower.contains("old_content") || lower.contains("old content") {
            "edit_mismatch"
        } else if lower.contains("no such file") || lower.contains("not found") {
            "file_not_found"
        } else if lower.contains("syntax") {
            "syntax_error"
        } else if lower.contains("429") || lower.contains("rate limit") {
            "rate_limit"
        } else if lower.contains("timeout") || lower.contains("timed out") {
            "timeout"
        } else {
            "generic"
        }
    }

    /// Check if the iteration limit has been reached.
    pub fn check_iteration_limit(&self, iteration: usize) -> bool {
        match self.config.max_iterations {
            Some(max) => iteration > max,
            None => false,
        }
    }

    /// Check if the last tool calls were all read-only and succeeded.
    ///
    /// Used to skip the thinking phase when the previous turn only did
    /// information gathering (no state changes to re-plan around).
    /// Mirrors Python's `IterationMixin._last_tools_were_readonly()`.
    pub fn should_skip_thinking(&self, messages: &[Value]) -> bool {
        let mut found_tools = false;
        // Collect tool names from the most recent assistant tool_calls
        let _last_assistant_tools: Vec<String> = Vec::new();

        for msg in messages.iter().rev() {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            match role {
                "tool" => {
                    let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                    let tool_name = msg.get("name").and_then(|n| n.as_str()).unwrap_or("");

                    // If any tool errored, don't skip thinking
                    if content.starts_with("Error")
                        || content.to_lowercase().contains("\"success\": false")
                    {
                        return false;
                    }
                    if !tool_name.is_empty() && !self.readonly_tools.contains(tool_name) {
                        return false;
                    }
                    found_tools = true;
                }
                "assistant" if found_tools => {
                    // Check tool_calls in the assistant message for non-readonly tools
                    if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                        for tc in tcs {
                            if let Some(name) = tc
                                .get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                                && !self.readonly_tools.contains(name)
                            {
                                return false;
                            }
                        }
                    }
                    break;
                }
                "user" if found_tools => break,
                "user" | "assistant" => return false,
                _ => {}
            }
        }
        found_tools
    }

    /// Count the number of assistant messages with tool_calls in a subagent result.
    ///
    /// Used for shallow subagent detection. If a subagent only made <=1 tool
    /// call, the parent could have done it directly.
    pub fn count_subagent_tool_calls(messages: &[Value]) -> usize {
        messages
            .iter()
            .filter(|msg| {
                msg.get("role").and_then(|r| r.as_str()) == Some("assistant")
                    && msg.get("tool_calls").is_some()
                    && !msg
                        .get("tool_calls")
                        .and_then(|tc| tc.as_array())
                        .map(|a| a.is_empty())
                        .unwrap_or(true)
            })
            .count()
    }

    /// Generate a shallow subagent warning suffix if applicable.
    ///
    /// Returns `Some(warning)` if the subagent made <=1 tool calls, `None` otherwise.
    pub fn shallow_subagent_warning(result_messages: &[Value], success: bool) -> Option<String> {
        if !success {
            return None;
        }
        let tool_call_count = Self::count_subagent_tool_calls(result_messages);
        if tool_call_count <= 1 {
            Some(format!(
                "\n\n[SHALLOW SUBAGENT WARNING] This subagent only made \
                 {tool_call_count} tool call(s). Spawning a subagent for a task \
                 that requires ≤1 tool call is wasteful — you should have used a \
                 direct tool call instead. For future similar tasks, use read_file, \
                 search, or list_files directly rather than spawning a subagent."
            ))
        } else {
            None
        }
    }

    /// Run the full ReAct loop over a message history.
    ///
    /// This is the main entry point. It takes initial messages and iteratively
    /// calls the LLM, processes tool calls, and manages the conversation until
    /// completion, interruption, or iteration limit.
    ///
    /// Tool execution is dispatched via the `ToolRegistry` with the given `ToolContext`.
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
    ) -> Result<AgentResult, AgentError>
    where
        M: TaskMonitor + ?Sized,
    {
        let mut iteration: usize = 0;
        let mut consecutive_no_tool_calls: usize = 0;
        let mut doom_detector = DoomLoopDetector::new();

        loop {
            iteration += 1;

            if self.check_iteration_limit(iteration) {
                info!(iteration, "Max iterations reached");
                return Ok(AgentResult::fail(
                    "Max iterations reached without completion",
                    messages.clone(),
                ));
            }

            // Check for interrupt
            if let Some(monitor) = task_monitor
                && monitor.should_interrupt()
            {
                return Ok(AgentResult::interrupted(messages.clone()));
            }

            // Thinking phase (before action)
            // Mirrors Python's 3-step flow: think → critique → refine → inject
            let skip_thinking = self.should_skip_thinking(messages);
            if self.config.thinking_level.is_enabled() && !skip_thinking {
                // Build analysis prompt based on original task
                let analysis_prompt = self.config.original_task.as_deref().map(|task| {
                    format!(
                        "The user's original request: {task}\n\n\
                         Analyze the full context and provide your reasoning for the \
                         next step. Keep the user's complete original request in mind \
                         — if it has multiple parts, ensure you are working toward ALL \
                         parts, not just the first.\n\n\
                         IMPORTANT: If your next step involves reading or searching \
                         multiple files to understand code structure, architecture, or \
                         patterns, you MUST delegate to Code-Explorer rather than doing \
                         it yourself. Only use direct read_file/search for known, \
                         specific targets (1-2 files)."
                    )
                });

                let thinking_payload = caller.build_thinking_payload(
                    messages,
                    self.config.thinking_system_prompt.as_deref(),
                    analysis_prompt.as_deref(),
                );
                debug!(iteration, "Running thinking phase");

                match http_client.post_json(&thinking_payload, None).await {
                    Ok(thinking_result) if thinking_result.success => {
                        if let Some(ref body) = thinking_result.body {
                            let thinking_resp = caller.parse_thinking_response(body);
                            if let Some(ref trace) = thinking_resp.content {
                                if let Some(cb) = event_callback {
                                    cb.on_thinking(trace);
                                }

                                // The trace to inject — may be refined by critique
                                let mut final_trace = trace.clone();

                                // Critique + refinement phase (High level only)
                                if self.config.thinking_level.use_critique() {
                                    let critique_system = embedded::SYSTEM_CRITIQUE;
                                    let critique_payload =
                                        caller.build_critique_payload(trace, critique_system);

                                    if let Ok(critique_result) =
                                        http_client.post_json(&critique_payload, None).await
                                        && critique_result.success
                                        && let Some(ref cbody) = critique_result.body
                                    {
                                        let critique_resp = caller.parse_critique_response(cbody);
                                        if let Some(ref critique_text) = critique_resp.content {
                                            if let Some(cb) = event_callback {
                                                cb.on_critique(critique_text);
                                            }

                                            // Refinement step: use critique to
                                            // improve the thinking trace
                                            let thinking_sys = self
                                                .config
                                                .thinking_system_prompt
                                                .as_deref()
                                                .unwrap_or(embedded::SYSTEM_THINKING);
                                            let refine_payload = caller.build_refinement_payload(
                                                thinking_sys,
                                                trace,
                                                critique_text,
                                            );

                                            if let Ok(refine_result) =
                                                http_client.post_json(&refine_payload, None).await
                                                && refine_result.success
                                                && let Some(ref rbody) = refine_result.body
                                            {
                                                let refine_resp =
                                                    caller.parse_thinking_response(rbody);
                                                if let Some(ref refined) = refine_resp.content {
                                                    if let Some(cb) = event_callback {
                                                        cb.on_thinking_refined(refined);
                                                    }
                                                    final_trace = refined.clone();
                                                }
                                            }
                                        }
                                    }
                                }

                                // Inject thinking trace as context for the action call
                                // Uses Python's stronger wording from thinking_trace_reminder
                                messages.push(serde_json::json!({
                                    "role": "user",
                                    "content": format!(
                                        "<thinking_trace>\n{final_trace}\n</thinking_trace>\n\n\
                                         You MUST follow the action plan in your thinking \
                                         trace above. Execute exactly the next step it \
                                         describes — do not skip ahead or choose a \
                                         different approach."
                                    ),
                                    "_thinking": true
                                }));
                            }
                        }
                    }
                    Ok(_) => {
                        debug!(iteration, "Thinking call returned non-success, skipping");
                    }
                    Err(e) => {
                        warn!(iteration, error = %e, "Thinking phase failed, proceeding to action");
                    }
                }
            }

            // Build payload and send via HttpClient
            let payload = caller.build_action_payload(messages, tool_schemas);
            debug!(iteration, model = %payload["model"], "ReAct iteration");

            let http_result = http_client
                .post_json(&payload, None)
                .await
                .map_err(|e| AgentError::LlmError(e.to_string()))?;

            if http_result.interrupted {
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

            // Track token usage
            if let Some(monitor) = task_monitor
                && let Some(ref usage) = response.usage
                && let Some(total) = usage.get("total_tokens").and_then(|t| t.as_u64())
            {
                monitor.update_tokens(total);
            }

            // Process the iteration
            let turn = self.process_iteration(
                &response,
                messages,
                iteration,
                &mut consecutive_no_tool_calls,
            )?;

            match turn {
                TurnResult::Interrupted => {
                    return Ok(AgentResult::interrupted(messages.clone()));
                }
                TurnResult::MaxIterations => {
                    return Ok(AgentResult::fail(
                        "Max iterations reached without completion",
                        messages.clone(),
                    ));
                }
                TurnResult::Complete { content, status } => {
                    if let Some(cb) = event_callback {
                        cb.on_agent_chunk(&content);
                    }
                    let mut result = AgentResult::ok(content, messages.clone());
                    result.completion_status = status;
                    return Ok(result);
                }
                TurnResult::ToolCall { tool_calls } => {
                    // Doom-loop detection
                    let (doom_action, doom_warning) = doom_detector.check(&tool_calls);
                    match doom_action {
                        DoomLoopAction::ForceStop => {
                            warn!(
                                nudge_count = doom_detector.nudge_count(),
                                "Doom loop force-stop: {}", doom_warning
                            );
                            return Ok(AgentResult::fail(
                                format!("Stopped: {doom_warning}"),
                                messages.clone(),
                            ));
                        }
                        DoomLoopAction::Notify => {
                            warn!("Doom loop notify: {}", doom_warning);
                            // Inject a system nudge but continue
                            messages.push(serde_json::json!({
                                "role": "user",
                                "content": format!(
                                    "[SYSTEM] Warning: {doom_warning} \
                                     Please try a different approach."
                                )
                            }));
                        }
                        DoomLoopAction::Redirect => {
                            debug!("Doom loop redirect: {}", doom_warning);
                            messages.push(serde_json::json!({
                                "role": "user",
                                "content": format!(
                                    "[SYSTEM] {doom_warning} \
                                     Consider trying a different approach or tool."
                                )
                            }));
                        }
                        DoomLoopAction::None => {}
                    }

                    // Execute tool calls
                    for tc in &tool_calls {
                        // Check for task_complete
                        if Self::is_task_complete(tc) {
                            let (summary, status) = Self::extract_task_complete_args(tc);
                            let mut result = AgentResult::ok(summary, messages.clone());
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

                        let tool_call_id_str =
                            tc.get("id").and_then(|id| id.as_str()).unwrap_or("unknown");

                        if let Some(cb) = event_callback {
                            cb.on_tool_started(tool_call_id_str, tool_name);
                        }

                        let tool_result = tool_registry
                            .execute(tool_name, args_map, tool_context)
                            .await;

                        if let Some(cb) = event_callback {
                            cb.on_tool_finished(tool_call_id_str, tool_result.success);
                        }

                        // Convert ToolResult to the Value format expected by format_tool_result
                        let result_value = if tool_result.success {
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

                        let formatted = Self::format_tool_result(tool_name, &result_value);

                        messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id_str,
                            "name": tool_name,
                            "content": formatted,
                        }));

                        // Check for interrupt between tool executions
                        if let Some(monitor) = task_monitor
                            && monitor.should_interrupt()
                        {
                            return Ok(AgentResult::interrupted(messages.clone()));
                        }
                    }
                }
                TurnResult::Continue => {
                    // LLM returned failure, loop will retry
                }
            }
        }
    }

    /// Process a single iteration given an already-parsed LLM response.
    ///
    /// This is the preferred integration point. The caller makes the HTTP
    /// request, parses the response, then calls this method to determine
    /// the next action.
    pub fn process_iteration(
        &self,
        response: &LlmResponse,
        messages: &mut Vec<Value>,
        iteration: usize,
        consecutive_no_tool_calls: &mut usize,
    ) -> Result<TurnResult, AgentError> {
        if self.check_iteration_limit(iteration) {
            return Ok(TurnResult::MaxIterations);
        }

        if response.interrupted {
            return Ok(TurnResult::Interrupted);
        }

        if !response.success {
            return Err(AgentError::LlmError(
                response
                    .error
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        // Append assistant message to history
        if let Some(ref msg) = response.message {
            let raw_content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
            let mut assistant_msg = serde_json::json!({
                "role": "assistant",
                "content": raw_content,
            });
            if let Some(tool_calls) = msg.get("tool_calls")
                && !tool_calls.is_null()
            {
                assistant_msg["tool_calls"] = tool_calls.clone();
            }
            messages.push(assistant_msg);
        }

        let turn = self.process_response(response, *consecutive_no_tool_calls);

        match &turn {
            TurnResult::ToolCall { .. } => {
                *consecutive_no_tool_calls = 0;
            }
            TurnResult::Complete { .. } => {
                *consecutive_no_tool_calls += 1;
            }
            _ => {}
        }

        Ok(turn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_loop() -> ReactLoop {
        ReactLoop::new(ReactLoopConfig {
            max_iterations: Some(10),
            max_nudge_attempts: 3,
            max_todo_nudges: 4,
            ..Default::default()
        })
    }

    #[test]
    fn test_turn_result_equality() {
        assert_eq!(TurnResult::Continue, TurnResult::Continue);
        assert_eq!(TurnResult::Interrupted, TurnResult::Interrupted);
        assert_eq!(TurnResult::MaxIterations, TurnResult::MaxIterations);
        assert_ne!(TurnResult::Continue, TurnResult::Interrupted);
    }

    #[test]
    fn test_process_response_interrupted() {
        let rl = make_loop();
        let resp = LlmResponse::interrupted();
        let result = rl.process_response(&resp, 0);
        assert_eq!(result, TurnResult::Interrupted);
    }

    #[test]
    fn test_process_response_failed() {
        let rl = make_loop();
        let resp = LlmResponse::fail("API error");
        let result = rl.process_response(&resp, 0);
        assert_eq!(result, TurnResult::Continue);
    }

    #[test]
    fn test_process_response_no_tool_calls() {
        let rl = make_loop();
        let msg = serde_json::json!({"role": "assistant", "content": "All done"});
        let resp = LlmResponse::ok(Some("All done".to_string()), msg);
        let result = rl.process_response(&resp, 0);
        match result {
            TurnResult::Complete { content, status } => {
                assert_eq!(content, "All done");
                assert!(status.is_none());
            }
            _ => panic!("Expected Complete"),
        }
    }

    #[test]
    fn test_process_response_with_tool_calls() {
        let rl = make_loop();
        let msg = serde_json::json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "tc-1",
                "function": {"name": "read_file", "arguments": "{}"}
            }]
        });
        let resp = LlmResponse::ok(None, msg);
        let result = rl.process_response(&resp, 0);
        match result {
            TurnResult::ToolCall { tool_calls } => {
                assert_eq!(tool_calls.len(), 1);
            }
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_all_parallelizable_single_tool() {
        let rl = make_loop();
        let tcs = vec![serde_json::json!({
            "function": {"name": "read_file"}
        })];
        // Single tool is not parallelizable (needs > 1)
        assert!(!rl.all_parallelizable(&tcs));
    }

    #[test]
    fn test_all_parallelizable_multiple_read_only() {
        let rl = make_loop();
        let tcs = vec![
            serde_json::json!({"function": {"name": "read_file"}}),
            serde_json::json!({"function": {"name": "search"}}),
        ];
        assert!(rl.all_parallelizable(&tcs));
    }

    #[test]
    fn test_all_parallelizable_with_write_tool() {
        let rl = make_loop();
        let tcs = vec![
            serde_json::json!({"function": {"name": "read_file"}}),
            serde_json::json!({"function": {"name": "write_file"}}),
        ];
        assert!(!rl.all_parallelizable(&tcs));
    }

    #[test]
    fn test_all_parallelizable_with_task_complete() {
        let rl = make_loop();
        let tcs = vec![
            serde_json::json!({"function": {"name": "read_file"}}),
            serde_json::json!({"function": {"name": "task_complete"}}),
        ];
        assert!(!rl.all_parallelizable(&tcs));
    }

    #[test]
    fn test_is_task_complete() {
        let tc = serde_json::json!({
            "function": {"name": "task_complete", "arguments": "{}"}
        });
        assert!(ReactLoop::is_task_complete(&tc));

        let tc2 = serde_json::json!({
            "function": {"name": "read_file", "arguments": "{}"}
        });
        assert!(!ReactLoop::is_task_complete(&tc2));
    }

    #[test]
    fn test_extract_task_complete_args() {
        let tc = serde_json::json!({
            "function": {
                "name": "task_complete",
                "arguments": "{\"summary\": \"All done\", \"status\": \"success\"}"
            }
        });
        let (summary, status) = ReactLoop::extract_task_complete_args(&tc);
        assert_eq!(summary, "All done");
        assert_eq!(status, "success");
    }

    #[test]
    fn test_extract_task_complete_args_defaults() {
        let tc = serde_json::json!({
            "function": {"name": "task_complete", "arguments": "{}"}
        });
        let (summary, status) = ReactLoop::extract_task_complete_args(&tc);
        assert_eq!(summary, "Task completed");
        assert_eq!(status, "success");
    }

    #[test]
    fn test_format_tool_result_success() {
        let result = serde_json::json!({"success": true, "output": "file contents"});
        let formatted = ReactLoop::format_tool_result("read_file", &result);
        assert_eq!(formatted, "file contents");
    }

    #[test]
    fn test_format_tool_result_success_with_status() {
        let result = serde_json::json!({
            "success": true,
            "output": "done",
            "completion_status": "partial"
        });
        let formatted = ReactLoop::format_tool_result("write_file", &result);
        assert_eq!(formatted, "[completion_status=partial]\ndone");
    }

    #[test]
    fn test_format_tool_result_failure() {
        let result = serde_json::json!({"success": false, "error": "file not found"});
        let formatted = ReactLoop::format_tool_result("read_file", &result);
        assert_eq!(formatted, "Error in read_file: file not found");
    }

    #[test]
    fn test_classify_error_permission() {
        assert_eq!(
            ReactLoop::classify_error("Permission denied: /etc"),
            "permission_error"
        );
    }

    #[test]
    fn test_classify_error_edit_mismatch() {
        assert_eq!(
            ReactLoop::classify_error("old_content not found in file"),
            "edit_mismatch"
        );
    }

    #[test]
    fn test_classify_error_file_not_found() {
        assert_eq!(
            ReactLoop::classify_error("No such file or directory"),
            "file_not_found"
        );
    }

    #[test]
    fn test_classify_error_syntax() {
        assert_eq!(
            ReactLoop::classify_error("SyntaxError: unexpected token"),
            "syntax_error"
        );
    }

    #[test]
    fn test_classify_error_rate_limit() {
        assert_eq!(
            ReactLoop::classify_error("429 Too Many Requests"),
            "rate_limit"
        );
    }

    #[test]
    fn test_classify_error_timeout() {
        assert_eq!(ReactLoop::classify_error("Request timed out"), "timeout");
    }

    #[test]
    fn test_classify_error_generic() {
        assert_eq!(ReactLoop::classify_error("Something went wrong"), "generic");
    }

    #[test]
    fn test_check_iteration_limit_unlimited() {
        let rl = ReactLoop::new(ReactLoopConfig {
            max_iterations: None,
            ..Default::default()
        });
        assert!(!rl.check_iteration_limit(1));
        assert!(!rl.check_iteration_limit(1000));
    }

    #[test]
    fn test_check_iteration_limit_bounded() {
        let rl = make_loop();
        assert!(!rl.check_iteration_limit(10)); // At limit
        assert!(rl.check_iteration_limit(11)); // Over limit
    }

    #[test]
    fn test_process_iteration_max_iterations() {
        let rl = make_loop();
        let resp = LlmResponse::ok(Some("hello".into()), serde_json::json!({}));
        let mut messages = vec![];
        let mut no_tools = 0;
        let result = rl.process_iteration(&resp, &mut messages, 11, &mut no_tools);
        assert!(matches!(result, Ok(TurnResult::MaxIterations)));
    }

    #[test]
    fn test_process_iteration_interrupted() {
        let rl = make_loop();
        let resp = LlmResponse::interrupted();
        let mut messages = vec![];
        let mut no_tools = 0;
        let result = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
        assert!(matches!(result, Ok(TurnResult::Interrupted)));
    }

    #[test]
    fn test_process_iteration_failed() {
        let rl = make_loop();
        let resp = LlmResponse::fail("error");
        let mut messages = vec![];
        let mut no_tools = 0;
        let result = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
        assert!(matches!(result, Err(AgentError::LlmError(_))));
    }

    #[test]
    fn test_process_iteration_appends_message() {
        let rl = make_loop();
        let msg = serde_json::json!({"role": "assistant", "content": "hi"});
        let resp = LlmResponse::ok(Some("hi".into()), msg);
        let mut messages = vec![];
        let mut no_tools = 0;
        let _ = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "assistant");
    }

    #[test]
    fn test_process_iteration_increments_no_tool_counter() {
        let rl = make_loop();
        let msg = serde_json::json!({"role": "assistant", "content": "done"});
        let resp = LlmResponse::ok(Some("done".into()), msg);
        let mut messages = vec![];
        let mut no_tools = 0;
        let _ = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
        assert_eq!(no_tools, 1);
    }

    #[test]
    fn test_process_iteration_resets_no_tool_counter_on_tool_call() {
        let rl = make_loop();
        let msg = serde_json::json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{"id": "1", "function": {"name": "read_file", "arguments": "{}"}}]
        });
        let resp = LlmResponse::ok(None, msg);
        let mut messages = vec![];
        let mut no_tools = 5;
        let _ = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
        assert_eq!(no_tools, 0);
    }

    #[test]
    fn test_default_config() {
        let config = ReactLoopConfig::default();
        assert!(config.max_iterations.is_none());
        assert_eq!(config.max_nudge_attempts, 3);
        assert_eq!(config.max_todo_nudges, 4);
    }

    // --- Thinking skip heuristic tests ---

    #[test]
    fn test_should_skip_thinking_after_readonly() {
        let rl = make_loop();
        let messages = vec![
            serde_json::json!({"role": "user", "content": "read a file"}),
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "1", "function": {"name": "read_file", "arguments": "{}"}}]
            }),
            serde_json::json!({"role": "tool", "name": "read_file", "content": "file contents", "tool_call_id": "1"}),
        ];
        assert!(rl.should_skip_thinking(&messages));
    }

    #[test]
    fn test_should_not_skip_thinking_after_write() {
        let rl = make_loop();
        let messages = vec![
            serde_json::json!({"role": "user", "content": "edit a file"}),
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "1", "function": {"name": "edit_file", "arguments": "{}"}}]
            }),
            serde_json::json!({"role": "tool", "name": "edit_file", "content": "ok", "tool_call_id": "1"}),
        ];
        assert!(!rl.should_skip_thinking(&messages));
    }

    #[test]
    fn test_should_not_skip_thinking_on_error() {
        let rl = make_loop();
        let messages = vec![
            serde_json::json!({"role": "user", "content": "read"}),
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "1", "function": {"name": "read_file", "arguments": "{}"}}]
            }),
            serde_json::json!({"role": "tool", "name": "read_file", "content": "Error: file not found", "tool_call_id": "1"}),
        ];
        assert!(!rl.should_skip_thinking(&messages));
    }

    #[test]
    fn test_should_not_skip_thinking_no_tools() {
        let rl = make_loop();
        let messages = vec![serde_json::json!({"role": "user", "content": "hello"})];
        assert!(!rl.should_skip_thinking(&messages));
    }

    #[test]
    fn test_should_skip_thinking_multiple_readonly() {
        let rl = make_loop();
        let messages = vec![
            serde_json::json!({"role": "user", "content": "search"}),
            serde_json::json!({"role": "assistant", "content": null, "tool_calls": [
                {"id": "1", "function": {"name": "read_file", "arguments": "{}"}},
                {"id": "2", "function": {"name": "search", "arguments": "{}"}}
            ]}),
            serde_json::json!({"role": "tool", "name": "read_file", "content": "ok", "tool_call_id": "1"}),
            serde_json::json!({"role": "tool", "name": "search", "content": "results", "tool_call_id": "2"}),
        ];
        assert!(rl.should_skip_thinking(&messages));
    }

    // --- Shallow subagent detection tests ---

    #[test]
    fn test_shallow_subagent_no_tools() {
        let messages = vec![
            serde_json::json!({"role": "system", "content": "You are..."}),
            serde_json::json!({"role": "user", "content": "do something"}),
            serde_json::json!({"role": "assistant", "content": "Done without tools."}),
        ];
        assert_eq!(ReactLoop::count_subagent_tool_calls(&messages), 0);
        let warning = ReactLoop::shallow_subagent_warning(&messages, true);
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("SHALLOW SUBAGENT WARNING"));
    }

    #[test]
    fn test_shallow_subagent_one_tool() {
        let messages = vec![
            serde_json::json!({"role": "assistant", "content": null, "tool_calls": [
                {"id": "1", "function": {"name": "read_file", "arguments": "{}"}}
            ]}),
            serde_json::json!({"role": "tool", "name": "read_file", "content": "ok"}),
            serde_json::json!({"role": "assistant", "content": "Here is the file."}),
        ];
        assert_eq!(ReactLoop::count_subagent_tool_calls(&messages), 1);
        assert!(ReactLoop::shallow_subagent_warning(&messages, true).is_some());
    }

    #[test]
    fn test_not_shallow_subagent_many_tools() {
        let messages = vec![
            serde_json::json!({"role": "assistant", "content": null, "tool_calls": [
                {"id": "1", "function": {"name": "read_file", "arguments": "{}"}}
            ]}),
            serde_json::json!({"role": "tool", "name": "read_file", "content": "ok"}),
            serde_json::json!({"role": "assistant", "content": null, "tool_calls": [
                {"id": "2", "function": {"name": "edit_file", "arguments": "{}"}}
            ]}),
            serde_json::json!({"role": "tool", "name": "edit_file", "content": "ok"}),
            serde_json::json!({"role": "assistant", "content": "Done."}),
        ];
        assert_eq!(ReactLoop::count_subagent_tool_calls(&messages), 2);
        assert!(ReactLoop::shallow_subagent_warning(&messages, true).is_none());
    }

    #[test]
    fn test_shallow_subagent_failed_no_warning() {
        let messages = vec![serde_json::json!({"role": "assistant", "content": "I failed."})];
        assert!(ReactLoop::shallow_subagent_warning(&messages, false).is_none());
    }

    // --- Thinking level configuration tests ---

    #[test]
    fn test_config_thinking_level_default() {
        let config = ReactLoopConfig::default();
        assert_eq!(config.thinking_level, ThinkingLevel::Medium);
        assert!(config.thinking_level.is_enabled());
        assert!(!config.thinking_level.use_critique());
    }

    #[test]
    fn test_config_thinking_level_off_skips_thinking() {
        let config = ReactLoopConfig {
            thinking_level: ThinkingLevel::Off,
            ..Default::default()
        };
        assert!(!config.thinking_level.is_enabled());
    }

    #[test]
    fn test_config_thinking_level_high_enables_critique() {
        let config = ReactLoopConfig {
            thinking_level: ThinkingLevel::High,
            ..Default::default()
        };
        assert!(config.thinking_level.is_enabled());
        assert!(config.thinking_level.use_critique());
    }

    #[test]
    fn test_thinking_skipped_after_readonly_tools() {
        // When last tools were readonly, should_skip_thinking returns true
        // meaning thinking won't run even if level is enabled
        let rl = ReactLoop::new(ReactLoopConfig {
            thinking_level: ThinkingLevel::Medium,
            ..Default::default()
        });
        let messages = vec![
            serde_json::json!({"role": "user", "content": "read something"}),
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "1", "function": {"name": "read_file", "arguments": "{}"}}]
            }),
            serde_json::json!({"role": "tool", "name": "read_file", "content": "ok", "tool_call_id": "1"}),
        ];
        assert!(rl.should_skip_thinking(&messages));
    }

    #[test]
    fn test_thinking_not_skipped_after_write_tools() {
        let rl = ReactLoop::new(ReactLoopConfig {
            thinking_level: ThinkingLevel::High,
            ..Default::default()
        });
        let messages = vec![
            serde_json::json!({"role": "user", "content": "edit something"}),
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "1", "function": {"name": "edit_file", "arguments": "{}"}}]
            }),
            serde_json::json!({"role": "tool", "name": "edit_file", "content": "ok", "tool_call_id": "1"}),
        ];
        assert!(!rl.should_skip_thinking(&messages));
    }

    #[test]
    fn test_critique_system_prompt_from_template() {
        let critique_prompt = embedded::SYSTEM_CRITIQUE;
        assert!(!critique_prompt.is_empty());
        assert!(
            critique_prompt.to_lowercase().contains("critique")
                || critique_prompt.to_lowercase().contains("critic")
        );
    }

    #[test]
    fn test_config_thinking_system_prompt() {
        let config = ReactLoopConfig {
            thinking_system_prompt: Some("custom thinking prompt".into()),
            original_task: Some("implement feature X".into()),
            ..Default::default()
        };
        assert_eq!(
            config.thinking_system_prompt.as_deref(),
            Some("custom thinking prompt")
        );
        assert_eq!(config.original_task.as_deref(), Some("implement feature X"));
    }
}
