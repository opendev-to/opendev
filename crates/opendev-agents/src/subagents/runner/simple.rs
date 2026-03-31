//! SimpleReactRunner — stripped-down loop for Explore subagents.

use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, info, warn};

use super::{RunnerContext, SubagentRunner};
use crate::react_loop::{PARALLELIZABLE_TOOLS, ReactLoop};
use crate::traits::{AgentError, AgentResult};

/// Normalize path arguments before emitting `on_tool_started` events.
///
/// Uses the canonical normalizer from `opendev-tools-core` so subagent event
/// displays show resolved paths (not raw LLM output like `src` or `./src`).
fn normalize_tool_args(
    tool_name: &str,
    args: HashMap<String, Value>,
    working_dir: &std::path::Path,
) -> HashMap<String, Value> {
    let wd = working_dir.to_string_lossy().to_string();
    opendev_tools_core::normalizer::normalize_params(tool_name, args, Some(&wd))
}

/// Structured summary of exploration actions taken by the subagent.
///
/// Extracted from message history and reused for both mid-loop observation
/// nudges and the final metadata footer appended to results.
struct ExplorationSummary {
    files_read: Vec<String>,
    searches: Vec<String>,
    dirs_listed: Vec<String>,
    commands_run: Vec<String>,
}

impl ExplorationSummary {
    /// Scan message history and extract exploration actions.
    fn from_messages(messages: &[Value]) -> Self {
        let mut files_read: Vec<String> = Vec::new();
        let mut searches: Vec<String> = Vec::new();
        let mut dirs_listed: Vec<String> = Vec::new();
        let mut commands_run: Vec<String> = Vec::new();

        for msg in messages {
            if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                continue;
            }
            let Some(tool_calls) = msg.get("tool_calls").and_then(|t| t.as_array()) else {
                continue;
            };
            for tc in tool_calls {
                let function = tc.get("function").cloned().unwrap_or_default();
                let name = function.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args_str = function
                    .get("arguments")
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}");
                let args: HashMap<String, Value> =
                    serde_json::from_str(args_str).unwrap_or_default();

                match name {
                    "read_file" => {
                        if let Some(path) = args.get("file_path").and_then(|v| v.as_str())
                            && !files_read.contains(&path.to_string())
                        {
                            files_read.push(path.to_string());
                        }
                    }
                    "grep" | "ast_grep" | "search" => {
                        if let Some(pattern) = args.get("pattern").and_then(|v| v.as_str()) {
                            let prefix = if name == "ast_grep" { "ast:" } else { "" };
                            searches.push(format!("{prefix}{pattern}"));
                        }
                    }
                    "list_files" => {
                        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                            dirs_listed.push(path.to_string());
                        } else {
                            dirs_listed.push(".".to_string());
                        }
                    }
                    "run_command" => {
                        if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                            commands_run.push(cmd.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        Self {
            files_read,
            searches,
            dirs_listed,
            commands_run,
        }
    }

    /// Total number of distinct tool calls tracked.
    fn total(&self) -> usize {
        self.files_read.len()
            + self.searches.len()
            + self.dirs_listed.len()
            + self.commands_run.len()
    }

    /// Format as a compact metadata footer for appending to subagent results.
    ///
    /// Ensures the parent always has a manifest of what was explored, even if
    /// the LLM's summary omits specific files. Caps lists at 30 entries.
    fn as_metadata_footer(&self) -> String {
        if self.total() == 0 {
            return String::new();
        }

        const MAX_ENTRIES: usize = 30;
        let mut footer = String::from("---\n## Exploration Metadata\n");

        fn format_list(items: &[String], max: usize) -> String {
            if items.len() <= max {
                items.join(", ")
            } else {
                let shown: Vec<_> = items[..max].to_vec();
                format!("{} [and {} more]", shown.join(", "), items.len() - max)
            }
        }

        if !self.files_read.is_empty() {
            footer.push_str(&format!(
                "- Files read ({}): {}\n",
                self.files_read.len(),
                format_list(&self.files_read, MAX_ENTRIES),
            ));
        }
        if !self.searches.is_empty() {
            footer.push_str(&format!(
                "- Searches ({}): {}\n",
                self.searches.len(),
                self.searches
                    .iter()
                    .take(MAX_ENTRIES)
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
            if self.searches.len() > MAX_ENTRIES {
                footer.push_str(&format!(
                    " [and {} more]",
                    self.searches.len() - MAX_ENTRIES
                ));
            }
        }
        if !self.dirs_listed.is_empty() {
            footer.push_str(&format!(
                "- Directories listed ({}): {}\n",
                self.dirs_listed.len(),
                format_list(&self.dirs_listed, MAX_ENTRIES),
            ));
        }
        if !self.commands_run.is_empty() {
            footer.push_str(&format!(
                "- Commands run ({}): {}\n",
                self.commands_run.len(),
                self.commands_run
                    .iter()
                    .take(MAX_ENTRIES)
                    .map(|c| format!("`{c}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
            if self.commands_run.len() > MAX_ENTRIES {
                footer.push_str(&format!(
                    " [and {} more]",
                    self.commands_run.len() - MAX_ENTRIES
                ));
            }
        }

        footer
    }

    /// Format as an observation nudge for mid-loop continuation.
    fn as_observation(&self, task: &str) -> String {
        let total = self.total();
        let mut obs = String::new();
        obs.push_str("## Exploration Status\n\n");
        obs.push_str(&format!("**Original task**: {task}\n\n"));
        obs.push_str(&format!("**Actions taken** ({total} tool calls):\n"));

        if !self.files_read.is_empty() {
            obs.push_str(&format!(
                "- Files read ({}): {}\n",
                self.files_read.len(),
                self.files_read.join(", ")
            ));
        }
        if !self.searches.is_empty() {
            obs.push_str(&format!(
                "- Searches ({}): {}\n",
                self.searches.len(),
                self.searches
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.dirs_listed.is_empty() {
            obs.push_str(&format!(
                "- Directories listed ({}): {}\n",
                self.dirs_listed.len(),
                self.dirs_listed.join(", ")
            ));
        }
        if !self.commands_run.is_empty() {
            obs.push_str(&format!(
                "- Commands run ({}): {}\n",
                self.commands_run.len(),
                self.commands_run
                    .iter()
                    .map(|c| format!("`{c}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        obs.push_str(
            "\nYou have explored very little so far. Review the original task — \
             if you need more evidence to give a confident answer, continue. \
             If you already have what you need, provide your final summary.\n",
        );

        obs
    }
}

/// A clean, minimal react loop for read-only exploration subagents.
///
/// Does ONLY: LLM call → parse → execute tools → repeat.
/// Skips: thinking/critique, doom loop detection, todo tracking,
/// completion nudges, consecutive-reads nudge, context compaction,
/// tool approval gates, cost tracking.
pub struct SimpleReactRunner {
    /// Maximum number of iterations (bounded for safety).
    max_iterations: usize,
    /// Maximum wall-clock duration for the entire run.
    max_duration: std::time::Duration,
}

impl SimpleReactRunner {
    /// Create a new simple runner with the given iteration limit and wall-clock cap.
    pub fn new(max_iterations: usize, max_duration: std::time::Duration) -> Self {
        Self {
            max_iterations,
            max_duration,
        }
    }

    /// Parse tool calls from an LLM response body.
    fn parse_tool_calls(body: &Value) -> Vec<Value> {
        body.get("choices")
            .and_then(|c| c.as_array())
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("tool_calls"))
            .and_then(|tcs| tcs.as_array())
            .cloned()
            .unwrap_or_default()
    }

    /// Extract content text from an LLM response body.
    fn parse_content(body: &Value) -> Option<String> {
        body.get("choices")
            .and_then(|c| c.as_array())
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string())
    }

    /// Extract the assistant message from an LLM response body.
    fn parse_assistant_message(body: &Value) -> Option<Value> {
        body.get("choices")
            .and_then(|c| c.as_array())
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .cloned()
    }

    /// Extract token usage from an LLM response body.
    fn parse_token_usage(body: &Value) -> (u64, u64) {
        let usage = body.get("usage");
        let input = usage
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        let output = usage
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        (input, output)
    }

    /// Extract tool name and parsed args from a tool call JSON object.
    fn extract_tool_info(tc: &Value) -> (String, String, HashMap<String, Value>) {
        let id = tc
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let function = tc.get("function").cloned().unwrap_or_default();
        let name = function
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        let args_str = function
            .get("arguments")
            .and_then(|a| a.as_str())
            .unwrap_or("{}");
        let args: HashMap<String, Value> = serde_json::from_str(args_str).unwrap_or_default();
        (id, name, args)
    }

    /// Make a final summary LLM call with tools stripped.
    ///
    /// Injects the `explorer_final_summary` reminder and forces a pure-text
    /// response. Falls back to `fallback_content` if the call fails.
    async fn final_summary_call(
        ctx: &RunnerContext<'_>,
        messages: &mut Vec<Value>,
        fallback_content: String,
    ) -> String {
        let summary_nudge = crate::prompts::reminders::get_reminder("explorer_final_summary", &[]);
        messages.push(serde_json::json!({
            "role": "user",
            "content": summary_nudge,
        }));

        let mut payload = ctx.caller.build_action_payload(messages, &[]);
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("tool_choice");
            obj.remove("tools");
            obj.remove("_reasoning_effort");
        }

        let noop_cb = opendev_http::streaming::FnStreamCallback(|_| {});
        match ctx
            .http_client
            .post_json_streaming(&payload, ctx.cancel, &noop_cb)
            .await
        {
            Ok(http_result) if http_result.success => {
                if let Some(body) = http_result.body {
                    if let Some(msg) = Self::parse_assistant_message(&body) {
                        messages.push(msg);
                    }
                    let (input_tokens, output_tokens) = Self::parse_token_usage(&body);
                    if let Some(cb) = ctx.event_callback {
                        cb.on_token_usage(input_tokens, output_tokens);
                    }
                    Self::parse_content(&body).unwrap_or(fallback_content)
                } else {
                    fallback_content
                }
            }
            _ => {
                warn!("SimpleReactRunner: final summary LLM call failed, using original content");
                fallback_content
            }
        }
    }

    /// Build an exploration observation from message history.
    ///
    /// Scans all assistant messages for tool calls and produces a structured
    /// summary of what has been explored. Used to give the model informed
    /// context when it tries to stop, so it can self-evaluate whether the
    /// exploration is sufficient.
    fn build_exploration_observation(messages: &[Value], task: &str) -> String {
        let summary = ExplorationSummary::from_messages(messages);
        summary.as_observation(task)
    }
}

#[async_trait]
impl SubagentRunner for SimpleReactRunner {
    async fn run(
        &self,
        ctx: &RunnerContext<'_>,
        messages: &mut Vec<Value>,
    ) -> Result<AgentResult, AgentError> {
        // Drain mailbox messages before starting (for team members)
        if let Some(mailbox) = ctx.mailbox
            && let Ok(msgs) = mailbox.receive()
        {
            super::inject_mailbox_messages(msgs, messages);
        }

        let parallelizable: std::collections::HashSet<&str> =
            PARALLELIZABLE_TOOLS.iter().copied().collect();
        let mut total_tool_calls = 0usize;
        let mut observation_count = 0usize;
        let mut auto_approved_patterns: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let start_time = Instant::now();

        // Extract the original task from the first user message for observation context
        let original_task = messages
            .iter()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
            .and_then(|m| m.get("content").and_then(|c| c.as_str()))
            .unwrap_or("explore the codebase")
            .to_string();

        for iteration in 1..=self.max_iterations {
            // Check cancellation
            if let Some(cancel) = ctx.cancel
                && cancel.is_cancelled()
            {
                info!(iteration, "SimpleReactRunner: cancelled");
                return Ok(AgentResult {
                    content: "Interrupted.".to_string(),
                    success: true,
                    interrupted: true,
                    backgrounded: false,
                    completion_status: None,
                    messages: messages.clone(),
                    partial_result: None,
                });
            }

            // Check wall-clock timeout
            if start_time.elapsed() > self.max_duration {
                info!(
                    iteration,
                    elapsed_secs = start_time.elapsed().as_secs(),
                    "SimpleReactRunner: wall-clock timeout reached"
                );
                break;
            }

            debug!(
                iteration,
                total_tool_calls, "SimpleReactRunner: calling LLM"
            );

            // Build payload and call LLM (streaming to get per-chunk idle timeout)
            let payload = ctx.caller.build_action_payload(messages, ctx.tool_schemas);

            // Debug log: outgoing subagent LLM request
            if let Some(logger) = ctx.debug_logger {
                let model = payload["model"].as_str().unwrap_or("unknown");
                let streaming = ctx.http_client.supports_streaming();
                logger.log_full(
                    "llm_request",
                    "subagent",
                    serde_json::json!({
                        "iteration": iteration,
                        "model": model,
                        "streaming": streaming,
                        "payload": payload,
                    }),
                );
            }

            let llm_start = std::time::Instant::now();
            let noop_cb = opendev_http::streaming::FnStreamCallback(|_| {});
            let http_result = ctx
                .http_client
                .post_json_streaming(&payload, ctx.cancel, &noop_cb)
                .await
                .map_err(|e| AgentError::LlmError(e.to_string()))?;
            let llm_latency_ms = llm_start.elapsed().as_millis() as u64;

            if !http_result.success {
                let status = http_result.status.unwrap_or(0);
                let body_text = http_result
                    .body
                    .as_ref()
                    .map(|b| b.to_string())
                    .unwrap_or_default();
                warn!(status, "SimpleReactRunner: LLM call failed");

                if let Some(logger) = ctx.debug_logger {
                    logger.log_full(
                        "llm_error",
                        "subagent",
                        serde_json::json!({
                            "iteration": iteration,
                            "status": status,
                            "error": body_text,
                        }),
                    );
                }

                // Retry on transient failures (stream timeouts, rate limits, server errors)
                if http_result.retryable || status == 429 || status >= 500 {
                    continue;
                }

                return Err(AgentError::LlmError(format!(
                    "LLM returned status {status}: {body_text}"
                )));
            }

            let body = match http_result.body {
                Some(b) => b,
                None => {
                    warn!("SimpleReactRunner: empty response body");
                    continue;
                }
            };

            // Emit token usage
            let (input_tokens, output_tokens) = Self::parse_token_usage(&body);

            // Debug log: incoming subagent LLM response
            if let Some(logger) = ctx.debug_logger {
                logger.log_full(
                    "llm_response",
                    "subagent",
                    serde_json::json!({
                        "iteration": iteration,
                        "latency_ms": llm_latency_ms,
                        "input_tokens": input_tokens,
                        "output_tokens": output_tokens,
                        "body": body,
                    }),
                );
            }
            if let Some(cb) = ctx.event_callback {
                cb.on_token_usage(input_tokens, output_tokens);
            }

            // Parse response
            let tool_calls = Self::parse_tool_calls(&body);
            let assistant_msg = Self::parse_assistant_message(&body);

            // Append assistant message to history
            if let Some(msg) = assistant_msg {
                messages.push(msg);
            }

            // If no tool calls → model wants to stop
            if tool_calls.is_empty() {
                let content = Self::parse_content(&body).unwrap_or_else(|| "Done.".to_string());

                // Observation-based continuation: if the model explored very little,
                // show it what it has done so far and let it decide whether to
                // continue. Skip entirely if it already made >= 10 tool calls.
                let should_observe = observation_count == 0 && total_tool_calls < 10;
                if should_observe {
                    observation_count += 1;
                    let observation = Self::build_exploration_observation(messages, &original_task);
                    debug!(
                        iteration,
                        total_tool_calls,
                        observation_count,
                        "SimpleReactRunner: injecting exploration observation",
                    );
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": observation,
                    }));
                    continue;
                }

                // If model made 0 tool calls even after observations, report failure
                if total_tool_calls == 0 {
                    return Ok(AgentResult {
                        content: "Exploration failed: no tool calls were made. The subagent could not find any files to explore in the working directory.".to_string(),
                        success: false,
                        interrupted: false,
                        backgrounded: false,
                        completion_status: None,
                        messages: messages.clone(),
                        partial_result: None,
                    });
                }

                // Final summary nudge: make one more LLM call with tools
                // stripped to force a comprehensive text summary, then append
                // a compact metadata footer of everything explored.
                let exploration = ExplorationSummary::from_messages(messages);

                debug!(
                    iteration,
                    tool_calls = total_tool_calls,
                    elapsed_secs = start_time.elapsed().as_secs(),
                    "SimpleReactRunner: requesting final summary (after {} observations)",
                    observation_count,
                );

                let final_content = Self::final_summary_call(ctx, messages, content).await;

                let metadata = exploration.as_metadata_footer();
                let mut result_content = final_content;
                if !metadata.is_empty() {
                    result_content.push_str("\n\n");
                    result_content.push_str(&metadata);
                }

                return Ok(AgentResult {
                    content: result_content,
                    success: true,
                    interrupted: false,
                    backgrounded: false,
                    completion_status: None,
                    messages: messages.clone(),
                    partial_result: None,
                });
            }

            // Execute tools — split into parallel batch (read-only) and sequential (side effects)
            {
                // Partition into parallelizable and sequential tool calls
                let mut parallel_infos: Vec<(String, String, HashMap<String, Value>)> = Vec::new();
                let mut sequential_tcs: Vec<&Value> = Vec::new();

                for tc in &tool_calls {
                    let (id, name, args) = Self::extract_tool_info(tc);
                    let args = normalize_tool_args(&name, args, &ctx.tool_context.working_dir);
                    if parallelizable.contains(name.as_str()) {
                        total_tool_calls += 1;
                        if let Some(cb) = ctx.event_callback {
                            cb.on_tool_started(&id, &name, &args);
                        }
                        parallel_infos.push((id, name, args));
                    } else {
                        sequential_tcs.push(tc);
                    }
                }

                // Execute parallel batch
                if !parallel_infos.is_empty() {
                    let futures: Vec<_> = parallel_infos
                        .iter()
                        .map(|(_, name, args)| {
                            ctx.tool_registry
                                .execute(name, args.clone(), ctx.tool_context)
                        })
                        .collect();

                    let results = futures::future::join_all(futures).await;

                    for ((id, name, _), result) in parallel_infos.iter().zip(results.iter()) {
                        if let Some(cb) = ctx.event_callback {
                            cb.on_tool_finished(id, result.success);
                        }
                        let result_value = serde_json::to_value(result).unwrap_or_default();
                        let content = ReactLoop::format_tool_result(name, &result_value);
                        messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": id,
                            "name": name,
                            "content": content,
                        }));
                    }
                }

                // Execute sequential tools
                for tc in sequential_tcs {
                    let (id, name, args) = Self::extract_tool_info(tc);
                    let mut args = normalize_tool_args(&name, args, &ctx.tool_context.working_dir);
                    total_tool_calls += 1;

                    // Emit tool started
                    if let Some(cb) = ctx.event_callback {
                        cb.on_tool_started(&id, &name, &args);
                    }

                    // Tool approval gate for run_command (mirrors ReactLoop behavior)
                    let auto_approved = if name == "run_command" {
                        let cmd = args
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
                        auto_approved_patterns.contains(&name)
                    };
                    let needs_approval = name == "run_command" && !auto_approved;
                    if needs_approval && let Some(approval_tx) = ctx.tool_approval_tx {
                        let command = args
                            .get("command")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
                        let req = opendev_runtime::ToolApprovalRequest {
                            tool_name: name.clone(),
                            command: command.clone(),
                            working_dir: ctx.tool_context.working_dir.display().to_string(),
                            response_tx: resp_tx,
                        };
                        if approval_tx.send(req).is_ok() {
                            match resp_rx.await {
                                Ok(d) if !d.approved => {
                                    let result_content = ReactLoop::format_tool_result(
                                        &name,
                                        &serde_json::json!({
                                            "success": false,
                                            "error": "Command denied by user"
                                        }),
                                    );
                                    messages.push(serde_json::json!({
                                        "role": "tool",
                                        "tool_call_id": id,
                                        "name": name,
                                        "content": result_content,
                                    }));
                                    if let Some(cb) = ctx.event_callback {
                                        cb.on_tool_result(
                                            &id,
                                            &name,
                                            "Command denied by user",
                                            false,
                                        );
                                        cb.on_tool_finished(&id, false);
                                    }
                                    continue;
                                }
                                Ok(d) => {
                                    if d.choice == "yes_remember" {
                                        if name == "run_command" {
                                            let prefix = opendev_runtime::extract_command_prefix(
                                                d.command.trim(),
                                            );
                                            debug!(
                                                prefix = %prefix,
                                                "Auto-approving command prefix for remainder of session"
                                            );
                                            auto_approved_patterns.insert(prefix);
                                        } else {
                                            auto_approved_patterns.insert(name.clone());
                                            debug!(
                                                tool = %name,
                                                "Auto-approving tool for remainder of session"
                                            );
                                        }
                                    }
                                    if d.command != command {
                                        args.insert(
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

                    let result = ctx
                        .tool_registry
                        .execute(&name, args, ctx.tool_context)
                        .await;

                    // Emit tool finished
                    if let Some(cb) = ctx.event_callback {
                        cb.on_tool_finished(&id, result.success);
                    }

                    // Format result as message
                    let result_value = serde_json::to_value(&result).unwrap_or_default();
                    let content = ReactLoop::format_tool_result(&name, &result_value);
                    messages.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": id,
                        "name": name,
                        "content": content,
                    }));
                }
            }
        }

        // Max iterations reached — attempt wind-down summary
        let exploration = ExplorationSummary::from_messages(messages);
        info!(
            iterations = self.max_iterations,
            tool_calls = total_tool_calls,
            elapsed_secs = start_time.elapsed().as_secs(),
            "SimpleReactRunner: max iterations reached — requesting wind-down"
        );

        // Fallback content from last assistant message
        let fallback = messages
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("Max iterations reached.")
            .to_string();

        // Inject summary prompt and make one final LLM call without tools
        let summary_prompt = crate::prompts::reminders::get_reminder("safety_limit_summary", &[]);
        messages.push(serde_json::json!({
            "role": "user",
            "content": summary_prompt,
        }));

        let mut payload = ctx.caller.build_action_payload(messages, &[]);
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("tool_choice");
            obj.remove("tools");
            obj.remove("_reasoning_effort");
        }

        let noop_cb = opendev_http::streaming::FnStreamCallback(|_| {});
        let wind_down_content = match ctx
            .http_client
            .post_json_streaming(&payload, ctx.cancel, &noop_cb)
            .await
        {
            Ok(http_result) if http_result.success => {
                if let Some(body) = http_result.body {
                    if let Some(msg) = Self::parse_assistant_message(&body) {
                        messages.push(msg);
                    }
                    let (input_tokens, output_tokens) = Self::parse_token_usage(&body);
                    if let Some(cb) = ctx.event_callback {
                        cb.on_token_usage(input_tokens, output_tokens);
                    }
                    Self::parse_content(&body).unwrap_or(fallback)
                } else {
                    fallback
                }
            }
            _ => {
                warn!("SimpleReactRunner: wind-down LLM call failed, using last content");
                fallback
            }
        };

        let metadata = exploration.as_metadata_footer();
        let mut result_content = format!(
            "[Max iterations ({}) reached — summary below]\n\n{}",
            self.max_iterations, wind_down_content
        );
        if !metadata.is_empty() {
            result_content.push_str("\n\n");
            result_content.push_str(&metadata);
        }

        Ok(AgentResult {
            content: result_content,
            success: true,
            interrupted: false,
            backgrounded: false,
            completion_status: None,
            messages: messages.clone(),
            partial_result: None,
        })
    }

    fn name(&self) -> &str {
        "SimpleReactRunner"
    }
}

#[cfg(test)]
#[path = "simple_tests.rs"]
mod tests;
