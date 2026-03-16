//! SubagentRunner trait and implementations.
//!
//! Defines a trait for react loop strategies so each subagent type
//! can have its own loop. `StandardReactRunner` wraps the existing
//! `ReactLoop` for General/Planner/Build agents, while `SimpleReactRunner`
//! provides a stripped-down loop for Code-Explorer.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::llm_calls::LlmCaller;
use crate::react_loop::{PARALLELIZABLE_TOOLS, ReactLoop, ReactLoopConfig};
use crate::traits::{AgentError, AgentEventCallback, AgentResult, TaskMonitor};
use opendev_http::adapted_client::AdaptedClient;
use opendev_runtime::ToolApprovalSender;
use opendev_tools_core::{ToolContext, ToolRegistry};
use tokio_util::sync::CancellationToken;

/// Dependencies bundled for the runner (avoids many-param functions).
pub struct RunnerContext<'a> {
    pub caller: &'a LlmCaller,
    pub http_client: &'a AdaptedClient,
    pub tool_schemas: &'a [Value],
    pub tool_registry: &'a ToolRegistry,
    pub tool_context: &'a ToolContext,
    pub event_callback: Option<&'a dyn AgentEventCallback>,
    pub cancel: Option<&'a CancellationToken>,
    pub tool_approval_tx: Option<&'a ToolApprovalSender>,
}

/// Trait for react loop strategies.
#[async_trait]
pub trait SubagentRunner: Send + Sync {
    /// Run the react loop over the given message history.
    async fn run(
        &self,
        ctx: &RunnerContext<'_>,
        messages: &mut Vec<Value>,
    ) -> Result<AgentResult, AgentError>;

    /// Name of this runner (for logging).
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// StandardReactRunner — wraps existing ReactLoop
// ---------------------------------------------------------------------------

/// Wraps the existing `ReactLoop` for General, Planner, Build agents.
///
/// Delegates to `ReactLoop::run()` with subagent-appropriate config
/// (no cost_tracker, no compactor, no todo_manager, no approval gates).
pub struct StandardReactRunner {
    config: ReactLoopConfig,
}

impl StandardReactRunner {
    /// Create a new standard runner with the given config.
    pub fn new(config: ReactLoopConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SubagentRunner for StandardReactRunner {
    async fn run(
        &self,
        ctx: &RunnerContext<'_>,
        messages: &mut Vec<Value>,
    ) -> Result<AgentResult, AgentError> {
        let react_loop = ReactLoop::new(self.config.clone());

        react_loop
            .run(
                ctx.caller,
                ctx.http_client,
                messages,
                ctx.tool_schemas,
                ctx.tool_registry,
                ctx.tool_context,
                None::<&dyn TaskMonitor>,
                ctx.event_callback,
                None, // no cost_tracker
                None, // no artifact_index
                None, // no compactor
                None, // no todo_manager
                ctx.cancel,
                ctx.tool_approval_tx,
            )
            .await
    }

    fn name(&self) -> &str {
        "StandardReactRunner"
    }
}

// ---------------------------------------------------------------------------
// SimpleReactRunner — stripped-down loop for Code-Explorer
// ---------------------------------------------------------------------------

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
                        if let Some(path) = args.get("file_path").and_then(|v| v.as_str()) {
                            if !files_read.contains(&path.to_string()) {
                                files_read.push(path.to_string());
                            }
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

    /// Normalize file path arguments against the working directory.
    ///
    /// LLMs often pass redundant paths like `project/src/main.rs` when cwd is
    /// already `/Users/.../project`. This strips the redundant prefix so both
    /// the tool execution and event display see clean relative paths.
    fn normalize_path_args(args: &mut HashMap<String, Value>, working_dir: &Path) {
        for key in &["file_path", "path"] {
            if let Some(val) = args
                .get(*key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
            {
                let resolved = Self::resolve_path(&val, working_dir);
                if resolved != val {
                    args.insert(key.to_string(), Value::String(resolved));
                }
            }
        }
    }

    /// Resolve a user-provided path against the working directory.
    ///
    /// Handles: `./prefix`, absolute paths, and redundant basename prefix.
    fn resolve_path(user_path: &str, working_dir: &Path) -> String {
        // Strip ./ prefix
        let path_str = user_path.strip_prefix("./").unwrap_or(user_path);
        let path = Path::new(path_str);

        if path.is_absolute() {
            // Absolute path: strip working dir prefix, then check for redundant basename
            if let Ok(rel) = path.strip_prefix(working_dir) {
                let rel_str = rel.to_string_lossy();
                // Check for redundant basename: /wd/basename/rest → rest
                if let Some(basename) = working_dir.file_name().and_then(|n| n.to_str()) {
                    if let Ok(inner) = rel.strip_prefix(basename) {
                        let inner_str = inner.to_string_lossy();
                        if !inner_str.is_empty() {
                            let joined = working_dir.join(&*inner_str);
                            if joined.exists()
                                || joined.parent().map(|p| p.is_dir()).unwrap_or(false)
                            {
                                return inner_str.to_string();
                            }
                        }
                    }
                }
                if !rel_str.is_empty() {
                    return rel_str.to_string();
                }
            }
            return path_str.to_string();
        }

        // Relative path: check if first component matches working dir basename
        if let Some(basename) = working_dir.file_name().and_then(|n| n.to_str()) {
            if let Some(rest) = path_str
                .strip_prefix(basename)
                .and_then(|r| r.strip_prefix('/'))
            {
                if !rest.is_empty() {
                    let joined = working_dir.join(rest);
                    if joined.exists() || joined.parent().map(|p| p.is_dir()).unwrap_or(false) {
                        return rest.to_string();
                    }
                }
            }
        }

        path_str.to_string()
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
            if let Some(cancel) = ctx.cancel {
                if cancel.is_cancelled() {
                    info!(iteration, "SimpleReactRunner: cancelled");
                    return Ok(AgentResult {
                        content: "Interrupted.".to_string(),
                        success: true,
                        interrupted: true,
                        completion_status: None,
                        messages: messages.clone(),
                        partial_result: None,
                    });
                }
            }

            debug!(
                iteration,
                total_tool_calls, "SimpleReactRunner: calling LLM"
            );

            // Build payload and call LLM
            let payload = ctx.caller.build_action_payload(messages, ctx.tool_schemas);
            let http_result = ctx
                .http_client
                .post_json(&payload, ctx.cancel)
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
                    let (id, name, mut args) = Self::extract_tool_info(tc);
                    Self::normalize_path_args(&mut args, &ctx.tool_context.working_dir);
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
                    let (id, name, mut args) = Self::extract_tool_info(tc);
                    Self::normalize_path_args(&mut args, &ctx.tool_context.working_dir);
                    total_tool_calls += 1;

                    // Emit tool started
                    if let Some(cb) = ctx.event_callback {
                        cb.on_tool_started(&id, &name, &args);
                    }

                    // Tool approval gate for run_command (mirrors ReactLoop behavior)
                    let needs_approval =
                        name == "run_command" && !auto_approved_patterns.contains(&name);
                    if needs_approval {
                        if let Some(approval_tx) = ctx.tool_approval_tx {
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
                                            auto_approved_patterns.insert(name.clone());
                                            debug!(
                                                tool = %name,
                                                "Auto-approving tool for remainder of session"
                                            );
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

        // Max iterations reached
        let elapsed = start_time.elapsed();
        info!(
            iterations = self.max_iterations,
            tool_calls = total_tool_calls,
            elapsed_secs = elapsed.as_secs(),
            "SimpleReactRunner: max iterations reached"
        );

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
mod tests {
    use super::*;

    #[test]
    fn test_simple_runner_parse_tool_calls_empty() {
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello"
                }
            }]
        });
        let calls = SimpleReactRunner::parse_tool_calls(&body);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_simple_runner_parse_tool_calls_present() {
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "tc-1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"file_path\": \"/src/main.rs\"}"
                        }
                    }]
                }
            }]
        });
        let calls = SimpleReactRunner::parse_tool_calls(&body);
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn test_simple_runner_parse_content() {
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Here is the analysis."
                }
            }]
        });
        let content = SimpleReactRunner::parse_content(&body);
        assert_eq!(content.as_deref(), Some("Here is the analysis."));
    }

    #[test]
    fn test_simple_runner_parse_token_usage() {
        let body = serde_json::json!({
            "usage": {
                "prompt_tokens": 1000,
                "completion_tokens": 500
            }
        });
        let (input, output) = SimpleReactRunner::parse_token_usage(&body);
        assert_eq!(input, 1000);
        assert_eq!(output, 500);
    }

    #[test]
    fn test_simple_runner_extract_tool_info() {
        let tc = serde_json::json!({
            "id": "call_abc",
            "type": "function",
            "function": {
                "name": "read_file",
                "arguments": "{\"file_path\": \"/src/main.rs\"}"
            }
        });
        let (id, name, args) = SimpleReactRunner::extract_tool_info(&tc);
        assert_eq!(id, "call_abc");
        assert_eq!(name, "read_file");
        assert_eq!(
            args.get("file_path").and_then(|v| v.as_str()),
            Some("/src/main.rs")
        );
    }

    #[test]
    fn test_build_exploration_observation() {
        let messages = vec![
            serde_json::json!({"role": "system", "content": "You are an explorer."}),
            serde_json::json!({"role": "user", "content": "Explore the codebase"}),
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    {"id": "1", "function": {"name": "list_files", "arguments": "{\"path\": \"src\"}"}},
                    {"id": "2", "function": {"name": "read_file", "arguments": "{\"file_path\": \"src/main.rs\"}"}},
                    {"id": "3", "function": {"name": "read_file", "arguments": "{\"file_path\": \"Cargo.toml\"}"}}
                ]
            }),
            serde_json::json!({"role": "tool", "tool_call_id": "1", "content": "file list"}),
            serde_json::json!({"role": "tool", "tool_call_id": "2", "content": "fn main(){}"}),
            serde_json::json!({"role": "tool", "tool_call_id": "3", "content": "[package]"}),
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    {"id": "4", "function": {"name": "search", "arguments": "{\"pattern\": \"fn main\"}"}}
                ]
            }),
            serde_json::json!({"role": "tool", "tool_call_id": "4", "content": "matches"}),
        ];

        let obs =
            SimpleReactRunner::build_exploration_observation(&messages, "Explore the codebase");
        assert!(obs.contains("**Original task**: Explore the codebase"));
        assert!(obs.contains("4 tool calls"));
        assert!(obs.contains("Files read (2)"));
        assert!(obs.contains("src/main.rs"));
        assert!(obs.contains("Cargo.toml"));
        assert!(obs.contains("Searches (1)"));
        assert!(obs.contains("`fn main`"));
        assert!(obs.contains("Directories listed (1)"));
        assert!(obs.contains("src"));
        // Should not contain commands since none were run
        assert!(!obs.contains("Commands run"));
    }

    #[test]
    fn test_build_exploration_observation_deduplicates_files() {
        let messages = vec![serde_json::json!({
            "role": "assistant",
            "tool_calls": [
                {"id": "1", "function": {"name": "read_file", "arguments": "{\"file_path\": \"src/main.rs\"}"}},
                {"id": "2", "function": {"name": "read_file", "arguments": "{\"file_path\": \"src/main.rs\"}"}}
            ]
        })];
        let obs = SimpleReactRunner::build_exploration_observation(&messages, "test");
        assert!(obs.contains("Files read (1)"));
    }

    #[test]
    fn test_simple_runner_name() {
        let runner = SimpleReactRunner::new(50);
        assert_eq!(runner.name(), "SimpleReactRunner");
    }

    #[test]
    fn test_standard_runner_name() {
        let runner = StandardReactRunner::new(ReactLoopConfig::default());
        assert_eq!(runner.name(), "StandardReactRunner");
    }
}
