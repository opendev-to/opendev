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

/// A clean, minimal react loop for read-only exploration subagents.
///
/// Does ONLY: LLM call → parse → execute tools → repeat.
/// Skips: thinking/critique, doom loop detection, todo tracking,
/// completion nudges, consecutive-reads nudge, context compaction,
/// tool approval gates, cost tracking.
pub struct SimpleReactRunner {
    /// Maximum number of iterations (bounded for safety).
    max_iterations: usize,
}

impl SimpleReactRunner {
    /// Create a new simple runner with the given iteration limit.
    pub fn new(max_iterations: usize) -> Self {
        Self { max_iterations }
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

    /// Build an exploration observation from message history.
    ///
    /// Scans all assistant messages for tool calls and produces a structured
    /// summary of what has been explored. Used to give the model informed
    /// context when it tries to stop, so it can self-evaluate whether the
    /// exploration is sufficient.
    fn build_exploration_observation(messages: &[Value], task: &str) -> String {
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
                    "search" => {
                        if let Some(pattern) = args.get("pattern").and_then(|v| v.as_str()) {
                            searches.push(pattern.to_string());
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

        let total = files_read.len() + searches.len() + dirs_listed.len() + commands_run.len();

        let mut obs = String::new();
        obs.push_str("## Exploration Status\n\n");
        obs.push_str(&format!("**Original task**: {task}\n\n"));
        obs.push_str(&format!("**Actions taken** ({total} tool calls):\n"));

        if !files_read.is_empty() {
            obs.push_str(&format!(
                "- Files read ({}): {}\n",
                files_read.len(),
                files_read.join(", ")
            ));
        }
        if !searches.is_empty() {
            obs.push_str(&format!(
                "- Searches ({}): {}\n",
                searches.len(),
                searches
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !dirs_listed.is_empty() {
            obs.push_str(&format!(
                "- Directories listed ({}): {}\n",
                dirs_listed.len(),
                dirs_listed.join(", ")
            ));
        }
        if !commands_run.is_empty() {
            obs.push_str(&format!(
                "- Commands run ({}): {}\n",
                commands_run.len(),
                commands_run
                    .iter()
                    .map(|c| format!("`{c}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        if total < 10 {
            obs.push_str(
                "\nYou have made very few tool calls. For a thorough exploration, \
                          you should investigate more files, directories, and patterns. \
                          Continue exploring — read key entry points, trace imports, \
                          search for important types and interfaces.\n",
            );
        } else {
            obs.push_str("\nBased on the original task and your exploration so far, decide:\n");
            obs.push_str("- If important areas remain unexplored, continue investigating.\n");
            obs.push_str("- If you have sufficient information, provide your final summary.\n");
        }

        obs
    }
}

#[async_trait]
impl SubagentRunner for SimpleReactRunner {
    async fn run(
        &self,
        ctx: &RunnerContext<'_>,
        messages: &mut Vec<Value>,
    ) -> Result<AgentResult, AgentError> {
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

            debug!(
                iteration,
                total_tool_calls, "SimpleReactRunner: calling LLM"
            );

            // Build payload and call LLM (streaming to get per-chunk idle timeout)
            let payload = ctx.caller.build_action_payload(messages, ctx.tool_schemas);
            let noop_cb = opendev_http::streaming::FnStreamCallback(|_| {});
            let http_result = ctx
                .http_client
                .post_json_streaming(&payload, ctx.cancel, &noop_cb)
                .await
                .map_err(|e| AgentError::LlmError(e.to_string()))?;

            if !http_result.success {
                let status = http_result.status.unwrap_or(0);
                let body_text = http_result
                    .body
                    .as_ref()
                    .map(|b| b.to_string())
                    .unwrap_or_default();
                warn!(status, "SimpleReactRunner: LLM call failed");

                // On rate limit or server error, retry (skip iteration)
                if status == 429 || status >= 500 {
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

                // Observation-based continuation: show the model what it has
                // explored and let it decide whether to continue.
                // - First observation is always given
                // - Second observation only if total_tool_calls < 10 (thin exploration)
                // - After 2 observations, accept the model's decision
                let should_observe =
                    observation_count == 0 || (observation_count == 1 && total_tool_calls < 10);
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

                let elapsed = start_time.elapsed();
                debug!(
                    iteration,
                    tool_calls = total_tool_calls,
                    elapsed_secs = elapsed.as_secs(),
                    "SimpleReactRunner: completed (model confirmed done after {} observations)",
                    observation_count,
                );
                return Ok(AgentResult {
                    content,
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
        let elapsed = start_time.elapsed();
        info!(
            iterations = self.max_iterations,
            tool_calls = total_tool_calls,
            elapsed_secs = elapsed.as_secs(),
            "SimpleReactRunner: max iterations reached — requesting wind-down"
        );

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
        match ctx
            .http_client
            .post_json_streaming(&payload, ctx.cancel, &noop_cb)
            .await
        {
            Ok(http_result) if http_result.success => {
                if let Some(body) = http_result.body
                    && let Some(content) = Self::parse_content(&body)
                {
                    let wind_down = format!(
                        "[Max iterations ({}) reached — summary below]\n\n{}",
                        self.max_iterations, content
                    );
                    return Ok(AgentResult {
                        content: wind_down,
                        success: true,
                        interrupted: false,
                        backgrounded: false,
                        completion_status: None,
                        messages: messages.clone(),
                        partial_result: None,
                    });
                }
            }
            Ok(_) | Err(_) => {
                warn!("SimpleReactRunner: wind-down LLM call failed, using last content");
            }
        }

        // Fallback: use last assistant content
        let last_content = messages
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("Max iterations reached.")
            .to_string();

        Ok(AgentResult {
            content: last_content,
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
