//! ReAct loop: reason → decide tool → execute → observe → loop.
//!
//! Mirrors `opendev/core/agents/main_agent/run_loop.py`.
//! The loop iterates up to a configurable maximum, executing tool calls
//! and feeding results back to the LLM until it completes or is interrupted.

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Instant;
use tracing::{Instrument, debug, info, info_span, warn};

use crate::agent_types::{AgentDefinition, PartialResult};
use crate::doom_loop::{DoomLoopAction, DoomLoopDetector, RecoveryAction};
use crate::llm_calls::LlmCaller;
use crate::response::ResponseCleaner;
use crate::subagents::spec::{PermissionAction, PermissionRule};
use crate::traits::{AgentError, AgentResult, LlmResponse, TaskMonitor};
use opendev_context::{ArtifactIndex, ContextCompactor, OptimizationLevel};
use opendev_http::adapted_client::AdaptedClient;
use opendev_runtime::{
    CostTracker, ThinkingLevel, TodoManager, TodoStatus, TokenUsage, play_finish_sound,
    summarize_tool_result,
};
use opendev_tools_core::{ToolContext, ToolRegistry, ToolResult};
use tokio_util::sync::CancellationToken;

/// Metrics for a single tool call execution.
#[derive(Debug, Clone)]
pub struct ToolCallMetric {
    /// Name of the tool that was called.
    pub tool_name: String,
    /// Wall-clock duration of the tool execution in milliseconds.
    pub duration_ms: u64,
    /// Whether the tool call succeeded.
    pub success: bool,
}

/// Per-iteration metrics collected during the ReAct loop.
#[derive(Debug, Clone, Default)]
pub struct IterationMetrics {
    /// 1-based iteration number.
    pub iteration: usize,
    /// Wall-clock latency of the LLM API call in milliseconds.
    pub llm_latency_ms: u64,
    /// Number of input (prompt) tokens consumed.
    pub input_tokens: u64,
    /// Number of output (completion) tokens generated.
    pub output_tokens: u64,
    /// Metrics for each tool call executed in this iteration.
    pub tool_calls: Vec<ToolCallMetric>,
    /// Total wall-clock duration of the iteration in milliseconds.
    pub total_duration_ms: u64,
}

/// Tools that are safe for parallel execution (read-only, no side effects).
pub static PARALLELIZABLE_TOOLS: &[&str] = &[
    "read_file",
    "list_files",
    "search",
    "fetch_url",
    "web_search",
    "capture_web_screenshot",
    "analyze_image",
    "list_todos",
    "search_tools",
    "find_symbol",
    "find_referencing_symbols",
];

use crate::prompts::embedded;
use crate::prompts::reminders::{
    MessageClass, append_directive, append_nudge, get_reminder, inject_system_message,
};

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
    "analyze_image",
    "capture_screenshot",
    "capture_web_screenshot",
    "list_sessions",
    "get_session_history",
    "list_subagents",
    "memory_search",
    "list_agents",
];

/// Read-only tool names for consecutive-reads detection.
/// When all tool calls in an iteration are from this set, the consecutive reads
/// counter increments. After 5 consecutive read-only iterations, a nudge is injected.
static READ_OPS: &[&str] = &[
    "read_file",
    "list_files",
    "search",
    "fetch_url",
    "web_search",
    "find_symbol",
    "list_todos",
    "read_pdf",
    "analyze_image",
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
    /// Optional agent definition — when set, the loop uses the agent's
    /// thinking/critique model overrides and thinking level.
    pub agent_definition: Option<AgentDefinition>,
    /// Per-agent permission rules for tool access control.
    /// Maps tool name patterns to permission rules (allow/deny/ask).
    /// When non-empty, each tool call is checked against these rules
    /// before execution.
    pub permission: HashMap<String, PermissionRule>,
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
            agent_definition: None,
            permission: HashMap::new(),
        }
    }
}

impl ReactLoopConfig {
    /// Return the effective thinking level, considering the agent definition override.
    pub fn effective_thinking_level(&self) -> ThinkingLevel {
        if let Some(ref def) = self.agent_definition {
            def.effective_thinking_level()
        } else {
            self.thinking_level
        }
    }

    /// Evaluate permission rules for a tool call.
    ///
    /// Returns `None` if no rules match (caller decides default behavior).
    /// `arg_pattern` is used for tools that have pattern-level rules (e.g. bash commands).
    pub fn evaluate_permission(
        &self,
        tool_name: &str,
        arg_pattern: &str,
    ) -> Option<PermissionAction> {
        use crate::subagents::spec::{glob_match, pattern_specificity};

        if self.permission.is_empty() {
            return None;
        }

        let mut best_match: Option<(PermissionAction, usize)> = None;

        for (tool_pattern, rule) in &self.permission {
            if !glob_match(tool_pattern, tool_name) {
                continue;
            }
            match rule {
                PermissionRule::Action(action) => {
                    let specificity = pattern_specificity(tool_pattern);
                    if best_match.as_ref().is_none_or(|(_, s)| specificity >= *s) {
                        best_match = Some((*action, specificity));
                    }
                }
                PermissionRule::Patterns(patterns) => {
                    for (pattern, action) in patterns {
                        if glob_match(pattern, arg_pattern) {
                            let specificity = pattern_specificity(pattern);
                            if best_match.as_ref().is_none_or(|(_, s)| specificity >= *s) {
                                best_match = Some((*action, specificity));
                            }
                        }
                    }
                }
            }
        }

        best_match.map(|(action, _)| action)
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
    /// Accumulated per-iteration metrics over the session.
    iteration_metrics: Mutex<Vec<IterationMetrics>>,
}

impl ReactLoop {
    /// Create a new ReAct loop with the given configuration.
    pub fn new(config: ReactLoopConfig) -> Self {
        Self {
            config,
            _cleaner: ResponseCleaner::new(),
            iteration_metrics: Mutex::new(Vec::new()),
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

    /// Return a snapshot of accumulated iteration metrics collected during `run()`.
    pub fn iteration_metrics(&self) -> Vec<IterationMetrics> {
        self.iteration_metrics.lock().unwrap().clone()
    }

    /// Clear accumulated iteration metrics.
    pub fn clear_metrics(&self) {
        self.iteration_metrics.lock().unwrap().clear();
    }

    /// Push a new iteration metrics entry.
    fn push_metrics(&self, metrics: IterationMetrics) {
        self.iteration_metrics.lock().unwrap().push(metrics);
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
            .get("result")
            .or_else(|| args.get("summary"))
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

        let base = if success {
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
        };

        // Append LLM-only suffix if present (hidden from UI, visible to LLM)
        if let Some(suffix) = result.get("llm_suffix").and_then(|s| s.as_str()) {
            format!("{base}\n\n{suffix}")
        } else {
            base
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
            // Skip injected thinking trace messages (fake assistant-user pair)
            if msg.get("_thinking").and_then(|v| v.as_bool()) == Some(true) {
                continue;
            }

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
    ///
    /// # Tracing
    ///
    /// This method emits structured tracing spans for observability:
    /// - `react_loop` — the outermost span for the full loop execution
    /// - `thinking_phase` — each thinking/critique/refine cycle
    /// - `llm_call` — each LLM HTTP request
    /// - `tool_execution` — each individual tool call
    ///
    /// Use `tracing-subscriber` with JSON formatting to capture structured logs:
    /// ```ignore
    /// tracing_subscriber::fmt()
    ///     .json()
    ///     .with_span_list(true)
    ///     .init();
    /// ```
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
        let mut doom_detector = DoomLoopDetector::new();

        // Per-subdirectory instruction injection tracker.
        // Initialized with the working dir as root; startup instruction
        // paths are discovered from existing instruction files.
        let mut subdir_tracker = {
            let startup_paths =
                opendev_context::discover_instruction_files(&tool_context.working_dir)
                    .into_iter()
                    .map(|f| f.path)
                    .collect::<Vec<_>>();
            opendev_context::SubdirInstructionTracker::new(
                tool_context.working_dir.clone(),
                &startup_paths,
            )
        };

        // Skill-driven model override: when a skill specifies model: in its
        // frontmatter, use that model for subsequent iterations until reset.
        let mut skill_model_override: Option<String> = None;

        // Session-level auto-approved tool patterns. When a user approves a
        // tool invocation with "yes_remember", the tool:args pattern is added
        // here so future identical invocations skip the approval prompt.
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

            // Auto-compaction: check context usage and apply staged optimization
            if let Some(comp) = compactor {
                let needs_llm = apply_staged_compaction(comp, messages);
                if needs_llm {
                    do_llm_compaction(comp, messages, caller, http_client).await;
                }
            }

            // Thinking phase (before action)
            // Mirrors Python's 3-step flow: think → critique → refine → inject
            let skip_thinking = self.should_skip_thinking(messages);
            let effective_level = self.config.effective_thinking_level();
            if effective_level.is_enabled() && !skip_thinking {
                let _thinking_span = info_span!(
                    "thinking_phase",
                    iteration = iteration,
                    level = %effective_level,
                );
                let _thinking_guard = _thinking_span.enter();
                drop(_thinking_guard);
                // Build analysis prompt based on original task and todo state
                let analysis_prompt = if let Some(mgr) = todo_manager
                    && let Ok(mgr) = mgr.lock()
                    && mgr.has_todos()
                {
                    let task = self.config.original_task.as_deref().unwrap_or("(unknown)");
                    Some(get_reminder(
                        "thinking_analysis_prompt_with_todos",
                        &[
                            ("original_task", task),
                            ("done_count", &mgr.completed_count().to_string()),
                            ("total_count", &mgr.total().to_string()),
                            ("todo_status", &mgr.format_status_sorted()),
                        ],
                    ))
                } else {
                    self.config.original_task.as_deref().map(|task| {
                        get_reminder("thinking_analysis_prompt", &[("original_task", task)])
                    })
                };

                let thinking_payload = caller.build_thinking_payload(
                    messages,
                    self.config.thinking_system_prompt.as_deref(),
                    analysis_prompt.as_deref(),
                );
                debug!(iteration, "Running thinking phase");

                match http_client.post_json(&thinking_payload, cancel).await {
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
                                if effective_level.use_critique() {
                                    let critique_system = embedded::SYSTEM_CRITIQUE;
                                    let critique_payload =
                                        caller.build_critique_payload(trace, critique_system);

                                    if let Ok(critique_result) =
                                        http_client.post_json(&critique_payload, cancel).await
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
                                                http_client.post_json(&refine_payload, cancel).await
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

                                // Remove any previous thinking trace message from messages
                                // to prevent accumulation across iterations.
                                messages.retain(|m| {
                                    m.get("_thinking").and_then(|v| v.as_bool()) != Some(true)
                                });

                                // Inject thinking trace as a tagged user message.
                                messages.push(serde_json::json!({
                                    "role": "user",
                                    "content": get_reminder("thinking_trace_reminder", &[("thinking_trace", &final_trace)]),
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

            // Build payload and send via HttpClient.
            // Apply skill model override if set (from invoke_skill metadata).
            let mut payload = caller.build_action_payload(messages, tool_schemas);
            if let Some(ref override_model) = skill_model_override {
                payload["model"] = serde_json::json!(override_model);
                debug!(iteration, model = %override_model, "Using skill model override");
            }
            debug!(iteration, model = %payload["model"], "ReAct iteration");

            let llm_start = Instant::now();
            let http_result = async {
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
            .await?;
            let llm_latency_ms = llm_start.elapsed().as_millis() as u64;

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

            // Track token usage
            if let Some(monitor) = task_monitor
                && let Some(ref usage) = response.usage
                && let Some(total) = usage.get("total_tokens").and_then(|t| t.as_u64())
            {
                monitor.update_tokens(total);
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
                if let Some(cb) = event_callback {
                    cb.on_context_usage(c.usage_pct());
                }
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
                    return Ok(AgentResult::interrupted(messages.clone()));
                }
                TurnResult::MaxIterations => {
                    iter_metrics.total_duration_ms = iter_start.elapsed().as_millis() as u64;
                    self.push_metrics(iter_metrics);
                    return Ok(AgentResult::fail(
                        "Max iterations reached without completion",
                        messages.clone(),
                    ));
                }
                TurnResult::Complete { content, status } => {
                    // Block completion when there are incomplete todos
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
                    if !completion_nudge_sent
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

                    // Accept completion
                    if let Some(cb) = event_callback {
                        cb.on_agent_chunk(&content);
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
                                "The agent was unable to make progress and has been \
                                 stopped. Please try rephrasing your request or \
                                 providing more specific guidance."
                                    .to_string(),
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
                                        "You appear to be stuck in a repeating loop. \
                                         Summarize what you have learned so far, discard \
                                         irrelevant details, and try a fundamentally \
                                         different approach.",
                                    );
                                }
                            }
                        }
                        DoomLoopAction::None => {}
                    }

                    // Execute tool calls
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
                            // Emit as agent chunk so TUI displays it
                            if let Some(cb) = event_callback {
                                cb.on_agent_chunk(&display_text);
                            }
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
                        let mut args_map: std::collections::HashMap<String, Value> = args_value
                            .as_object()
                            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default();

                        let tool_call_id_str =
                            tc.get("id").and_then(|id| id.as_str()).unwrap_or("unknown");

                        if let Some(cb) = event_callback {
                            cb.on_tool_started(tool_call_id_str, tool_name, &args_map);
                        }

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
                                        if let Some(cb) = event_callback {
                                            cb.on_tool_result(
                                                tool_call_id_str,
                                                tool_name,
                                                "Permission denied by agent rules",
                                                false,
                                            );
                                            cb.on_tool_finished(tool_call_id_str, false);
                                        }
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
                                                        if let Some(cb) = event_callback {
                                                            cb.on_tool_result(
                                                                tool_call_id_str,
                                                                tool_name,
                                                                "Tool call denied by user",
                                                                false,
                                                            );
                                                            cb.on_tool_finished(
                                                                tool_call_id_str,
                                                                false,
                                                            );
                                                        }
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
                        let auto_approved = auto_approved_patterns.contains(tool_name);
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
                                        if let Some(cb) = event_callback {
                                            cb.on_tool_result(
                                                tool_call_id_str,
                                                tool_name,
                                                "Command denied by user",
                                                false,
                                            );
                                            cb.on_tool_finished(tool_call_id_str, false);
                                        }
                                        continue;
                                    }
                                    Ok(d) => {
                                        // "yes_remember" — auto-approve this tool for rest of session
                                        if d.choice == "yes_remember" {
                                            auto_approved_patterns.insert(tool_name.to_string());
                                            debug!(
                                                tool = tool_name,
                                                "Auto-approving tool for remainder of session"
                                            );
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

                        if let Some(cb) = event_callback {
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
                                cb.on_tool_result(
                                    tool_call_id_str,
                                    tool_name,
                                    output_str,
                                    tool_result.success,
                                );
                            }
                            cb.on_tool_finished(tool_call_id_str, tool_result.success);
                        }

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
                                "read_file" | "edit_file" | "write_file" | "search"
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

                        completed_tool_count += 1;

                        // Check for interrupt between tool executions —
                        // preserve partial work (completed tool results
                        // already appended to messages above).
                        let interrupted_by_monitor =
                            task_monitor.is_some_and(|m| m.should_interrupt());
                        let interrupted_by_cancel = cancel.is_some_and(|c| c.is_cancelled());
                        if interrupted_by_monitor || interrupted_by_cancel {
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
                    }

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

/// Convert `Vec<Value>` messages to `Vec<ApiMessage>` for the compactor.
///
/// Only includes `Value::Object` entries; non-object values are skipped.
fn values_to_api_messages(values: &[Value]) -> Vec<opendev_context::compaction::ApiMessage> {
    values
        .iter()
        .filter_map(|v| v.as_object().cloned())
        .collect()
}

/// Apply staged context compaction based on current usage level.
///
/// Mirrors Python's `_maybe_compact()` — checks context usage percentage
/// and applies the appropriate optimization strategy:
/// - 70%: Warning only
/// - 80%: Mask old tool results with compact refs
/// - 85%: Prune old tool outputs
/// - 90%: Aggressive masking (fewer recent results preserved)
/// - 99%: Full compaction (summarize middle messages)
///
/// Returns `true` if LLM-powered compaction is needed (99% stage).
#[allow(clippy::ptr_arg)] // needs Vec for clear()/extend() in Compact branch caller
fn apply_staged_compaction(compactor: &Mutex<ContextCompactor>, messages: &mut Vec<Value>) -> bool {
    let api_msgs = values_to_api_messages(messages);
    let level = if let Ok(mut comp) = compactor.lock() {
        comp.check_usage(&api_msgs, "")
    } else {
        return false;
    };

    match level {
        OptimizationLevel::None | OptimizationLevel::Warning => false,
        OptimizationLevel::Mask | OptimizationLevel::Aggressive => {
            // Convert to ApiMessage, apply masking + early summarization, convert back
            let mut api_msgs = values_to_api_messages(messages);
            if let Ok(comp) = compactor.lock() {
                comp.summarize_verbose_tool_outputs(&mut api_msgs);
                comp.mask_old_observations(&mut api_msgs, level);
            }
            // Write masked messages back
            let mut api_idx = 0;
            for msg in messages.iter_mut() {
                if msg.is_object() && api_idx < api_msgs.len() {
                    *msg = Value::Object(api_msgs[api_idx].clone());
                    api_idx += 1;
                }
            }
            false
        }
        OptimizationLevel::Prune => {
            let mut api_msgs = values_to_api_messages(messages);
            if let Ok(comp) = compactor.lock() {
                comp.summarize_verbose_tool_outputs(&mut api_msgs);
                comp.mask_old_observations(&mut api_msgs, OptimizationLevel::Mask);
                comp.prune_old_tool_outputs(&mut api_msgs);
            }
            let mut api_idx = 0;
            for msg in messages.iter_mut() {
                if msg.is_object() && api_idx < api_msgs.len() {
                    *msg = Value::Object(api_msgs[api_idx].clone());
                    api_idx += 1;
                }
            }
            false
        }
        OptimizationLevel::Compact => true,
    }
}

/// Perform LLM-powered compaction: build payload, call the compact model,
/// and replace messages with the summarized version.
///
/// Falls back to `compact()` (basic string summarization) if the LLM call
/// fails or if no compact model is configured.
async fn do_llm_compaction(
    compactor: &Mutex<ContextCompactor>,
    messages: &mut Vec<Value>,
    caller: &LlmCaller,
    http_client: &AdaptedClient,
) {
    use crate::prompts::embedded::SYSTEM_COMPACTION;

    let api_msgs = values_to_api_messages(messages);
    let compact_model = &caller.config.model;

    // Try to build the LLM compaction payload
    let build_result = if let Ok(comp) = compactor.lock() {
        comp.build_compaction_payload(&api_msgs, SYSTEM_COMPACTION, compact_model)
    } else {
        None
    };

    let Some((payload, _middle_count, keep_recent)) = build_result else {
        // Too few messages or lock failed — fallback to basic compact
        if let Ok(mut comp) = compactor.lock() {
            let compacted = comp.compact(api_msgs, "");
            messages.clear();
            messages.extend(compacted.into_iter().map(Value::Object));
        }
        return;
    };

    // Call the LLM via the adapted client (uses provider adapters, auth, retries)
    let summary_text: Option<String> = match http_client.post_json(&payload, None).await {
        Ok(result) => result
            .body
            .as_ref()
            .and_then(|body| body.pointer("/choices/0/message/content"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        Err(e) => {
            warn!("LLM compaction request failed: {e}, using fallback");
            None
        }
    };

    let summary = match summary_text {
        Some(text) if !text.is_empty() => {
            info!(
                model = compact_model,
                summary_len = text.len(),
                "LLM compaction succeeded"
            );
            text
        }
        _ => {
            warn!("LLM compaction returned empty or failed, using fallback");
            ContextCompactor::fallback_summary(
                &api_msgs[1..api_msgs.len().saturating_sub(keep_recent)],
            )
        }
    };

    // Apply the compaction
    if let Ok(mut comp) = compactor.lock() {
        let compacted = comp.apply_llm_compaction(api_msgs, &summary, keep_recent);
        messages.clear();
        messages.extend(compacted.into_iter().map(Value::Object));
    }
}

/// Record file operations in the artifact index after successful tool execution.
///
/// Mirrors Python's `_record_artifact()` — tracks read/write/edit operations
/// so the artifact index survives compaction and the agent retains file awareness.
fn record_artifact(
    artifact_index: &Mutex<ArtifactIndex>,
    tool_name: &str,
    args: &Value,
    result: &ToolResult,
) {
    let file_path = match args.get("file_path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return,
    };

    let (operation, details) = match tool_name {
        "read_file" => {
            let line_count = result
                .output
                .as_deref()
                .map(|o| o.lines().count())
                .unwrap_or(0);
            ("read", format!("{line_count} lines"))
        }
        "write_file" => {
            let line_count = args
                .get("content")
                .and_then(|v| v.as_str())
                .map(|c| c.lines().count())
                .unwrap_or(0);
            ("created", format!("{line_count} lines"))
        }
        "edit_file" => ("modified", "edit".to_string()),
        _ => return,
    };

    if let Ok(mut index) = artifact_index.lock() {
        index.record(file_path, operation, &details);
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
                "arguments": "{\"result\": \"All done\", \"status\": \"success\"}"
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
    fn test_should_skip_thinking_ignores_thinking_trace() {
        // The thinking trace message (_thinking: true) should be invisible
        // to should_skip_thinking — it should look through it at the real messages.
        let rl = make_loop();
        // Readonly tools followed by thinking trace → still skip
        let messages = vec![
            serde_json::json!({"role": "user", "content": "read something"}),
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "1", "function": {"name": "read_file", "arguments": "{}"}}]
            }),
            serde_json::json!({"role": "tool", "name": "read_file", "content": "ok", "tool_call_id": "1"}),
            serde_json::json!({"role": "user", "content": "<thinking_trace>...</thinking_trace>", "_thinking": true}),
        ];
        assert!(rl.should_skip_thinking(&messages));
    }

    #[test]
    fn test_should_not_skip_thinking_with_trace_after_write() {
        // Write tool followed by thinking trace → don't skip
        let rl = make_loop();
        let messages = vec![
            serde_json::json!({"role": "user", "content": "edit something"}),
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "1", "function": {"name": "edit_file", "arguments": "{}"}}]
            }),
            serde_json::json!({"role": "tool", "name": "edit_file", "content": "ok", "tool_call_id": "1"}),
            serde_json::json!({"role": "user", "content": "<thinking_trace>...</thinking_trace>", "_thinking": true}),
        ];
        assert!(!rl.should_skip_thinking(&messages));
    }

    #[test]
    fn test_should_not_skip_thinking_only_trace_no_tools() {
        // Only thinking trace, no tool results → don't skip (retryable failure case)
        let rl = make_loop();
        let messages = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({"role": "user", "content": "<thinking_trace>...</thinking_trace>", "_thinking": true}),
        ];
        assert!(!rl.should_skip_thinking(&messages));
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

    // --- Iteration metrics tests ---

    #[test]
    fn test_iteration_metrics_default() {
        let metrics = IterationMetrics::default();
        assert_eq!(metrics.iteration, 0);
        assert_eq!(metrics.llm_latency_ms, 0);
        assert_eq!(metrics.input_tokens, 0);
        assert_eq!(metrics.output_tokens, 0);
        assert!(metrics.tool_calls.is_empty());
        assert_eq!(metrics.total_duration_ms, 0);
    }

    #[test]
    fn test_tool_call_metric() {
        let metric = ToolCallMetric {
            tool_name: "read_file".to_string(),
            duration_ms: 42,
            success: true,
        };
        assert_eq!(metric.tool_name, "read_file");
        assert_eq!(metric.duration_ms, 42);
        assert!(metric.success);
    }

    #[test]
    fn test_metrics_accumulation() {
        let rl = make_loop();

        // Initially empty
        assert!(rl.iteration_metrics().is_empty());

        // Push a metric
        rl.push_metrics(IterationMetrics {
            iteration: 1,
            llm_latency_ms: 100,
            input_tokens: 500,
            output_tokens: 200,
            tool_calls: vec![ToolCallMetric {
                tool_name: "read_file".to_string(),
                duration_ms: 10,
                success: true,
            }],
            total_duration_ms: 150,
        });

        let metrics = rl.iteration_metrics();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].iteration, 1);
        assert_eq!(metrics[0].llm_latency_ms, 100);
        assert_eq!(metrics[0].tool_calls.len(), 1);
        assert_eq!(metrics[0].tool_calls[0].tool_name, "read_file");
    }

    #[test]
    fn test_metrics_clear() {
        let rl = make_loop();
        rl.push_metrics(IterationMetrics {
            iteration: 1,
            ..Default::default()
        });
        assert_eq!(rl.iteration_metrics().len(), 1);

        rl.clear_metrics();
        assert!(rl.iteration_metrics().is_empty());
    }

    // --- Partial result preservation tests ---

    #[test]
    fn test_agent_result_interrupted_with_partial_content() {
        let messages = vec![
            serde_json::json!({"role": "user", "content": "do stuff"}),
            serde_json::json!({"role": "tool", "name": "read_file", "content": "file data", "tool_call_id": "tc-1"}),
        ];
        let mut result = AgentResult::interrupted(messages);
        // Simulate partial content preservation
        result.content = "Task interrupted by user (partial): I was analyzing...".to_string();
        assert!(result.interrupted);
        assert!(result.content.contains("partial"));
        assert!(result.content.contains("analyzing"));
        // Messages should include the completed tool result
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[1]["name"], "read_file");
    }

    // --- Permission enforcement tests ---

    #[test]
    fn test_evaluate_permission_empty_rules() {
        let config = ReactLoopConfig::default();
        assert!(config.evaluate_permission("run_command", "ls").is_none());
    }

    #[test]
    fn test_evaluate_permission_blanket_deny() {
        let mut config = ReactLoopConfig::default();
        config.permission.insert(
            "run_command".to_string(),
            PermissionRule::Action(PermissionAction::Deny),
        );
        assert_eq!(
            config.evaluate_permission("run_command", "ls"),
            Some(PermissionAction::Deny)
        );
    }

    #[test]
    fn test_evaluate_permission_blanket_allow() {
        let mut config = ReactLoopConfig::default();
        config.permission.insert(
            "read_file".to_string(),
            PermissionRule::Action(PermissionAction::Allow),
        );
        assert_eq!(
            config.evaluate_permission("read_file", ""),
            Some(PermissionAction::Allow)
        );
    }

    #[test]
    fn test_evaluate_permission_wildcard_tool_pattern() {
        let mut config = ReactLoopConfig::default();
        config.permission.insert(
            "*".to_string(),
            PermissionRule::Action(PermissionAction::Ask),
        );
        assert_eq!(
            config.evaluate_permission("write_file", ""),
            Some(PermissionAction::Ask)
        );
    }

    #[test]
    fn test_evaluate_permission_specific_overrides_wildcard() {
        let mut config = ReactLoopConfig::default();
        config.permission.insert(
            "*".to_string(),
            PermissionRule::Action(PermissionAction::Ask),
        );
        config.permission.insert(
            "read_file".to_string(),
            PermissionRule::Action(PermissionAction::Allow),
        );
        // Specific "read_file" should override wildcard "*"
        assert_eq!(
            config.evaluate_permission("read_file", ""),
            Some(PermissionAction::Allow)
        );
        // Wildcard still applies to other tools
        assert_eq!(
            config.evaluate_permission("write_file", ""),
            Some(PermissionAction::Ask)
        );
    }

    #[test]
    fn test_evaluate_permission_pattern_rules() {
        let mut patterns = HashMap::new();
        patterns.insert("*".to_string(), PermissionAction::Ask);
        patterns.insert("git *".to_string(), PermissionAction::Allow);
        patterns.insert("rm -rf *".to_string(), PermissionAction::Deny);

        let mut config = ReactLoopConfig::default();
        config.permission.insert(
            "run_command".to_string(),
            PermissionRule::Patterns(patterns),
        );

        assert_eq!(
            config.evaluate_permission("run_command", "git status"),
            Some(PermissionAction::Allow)
        );
        assert_eq!(
            config.evaluate_permission("run_command", "rm -rf /"),
            Some(PermissionAction::Deny)
        );
        assert_eq!(
            config.evaluate_permission("run_command", "ls -la"),
            Some(PermissionAction::Ask)
        );
    }

    #[test]
    fn test_evaluate_permission_no_match() {
        let mut config = ReactLoopConfig::default();
        config.permission.insert(
            "run_command".to_string(),
            PermissionRule::Action(PermissionAction::Deny),
        );
        // Different tool should not match
        assert!(config.evaluate_permission("read_file", "").is_none());
    }

    #[test]
    fn test_default_config_has_empty_permissions() {
        let config = ReactLoopConfig::default();
        assert!(config.permission.is_empty());
    }

    #[test]
    fn test_mcp_tool_needs_approval_gate() {
        // MCP tools (mcp__*) should be treated the same as run_command
        // for approval purposes
        let tool_name = "mcp__github__create_issue";
        let needs_approval = tool_name == "run_command" || tool_name.starts_with("mcp__");
        assert!(needs_approval);

        // Regular tools should not need the bash approval gate
        let tool_name = "read_file";
        let needs_approval = tool_name == "run_command" || tool_name.starts_with("mcp__");
        assert!(!needs_approval);
    }

    #[test]
    fn test_mcp_permission_rule_matches() {
        let mut config = ReactLoopConfig::default();
        config.permission.insert(
            "mcp__*".to_string(),
            PermissionRule::Action(PermissionAction::Ask),
        );
        // MCP tool should match the glob
        assert_eq!(
            config.evaluate_permission("mcp__sqlite__query", ""),
            Some(PermissionAction::Ask)
        );
        // Non-MCP tool should not match
        assert!(config.evaluate_permission("read_file", "").is_none());
    }

    #[test]
    fn test_mcp_permission_allow_specific() {
        let mut config = ReactLoopConfig::default();
        // Deny all MCP by default
        config.permission.insert(
            "mcp__*".to_string(),
            PermissionRule::Action(PermissionAction::Deny),
        );
        // But allow a specific MCP tool
        config.permission.insert(
            "mcp__sqlite__query".to_string(),
            PermissionRule::Action(PermissionAction::Allow),
        );
        // Specific rule should win (higher specificity)
        assert_eq!(
            config.evaluate_permission("mcp__sqlite__query", ""),
            Some(PermissionAction::Allow)
        );
        // Other MCP tools should be denied
        assert_eq!(
            config.evaluate_permission("mcp__github__push", ""),
            Some(PermissionAction::Deny)
        );
    }
}
