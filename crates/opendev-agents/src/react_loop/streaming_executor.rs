//! Streaming tool executor: starts read-only tools during LLM streaming.
//!
//! When the LLM streams a `FunctionCallDone` event for a read-only tool,
//! the executor immediately spawns a tokio task to execute it. This overlaps
//! tool execution with the remaining LLM generation, reducing per-iteration
//! latency by 30-60% on tool-heavy turns.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use opendev_http::streaming::{StreamCallback, StreamEvent};
use opendev_tools_core::{ToolContext, ToolRegistry};
use serde_json::Value;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// Result of a tool that was executed early (during streaming).
#[derive(Debug)]
pub(super) struct EarlyToolResult {
    #[allow(dead_code)]
    pub call_id: String,
    #[allow(dead_code)]
    pub tool_name: String,
    pub result: opendev_tools_core::ToolResult,
    pub duration_ms: u64,
}

/// A completed tool call parsed from stream events, ready for execution.
#[derive(Debug, Clone)]
struct CompletedToolCall {
    call_id: String,
    name: String,
    #[allow(dead_code)]
    arguments: String,
}

/// Pre-parsed arguments for write tools (avoids re-parsing after streaming).
#[derive(Debug, Clone)]
pub(super) struct PreparsedArgs {
    pub args_map: HashMap<String, Value>,
}

/// Executes read-only tools during LLM streaming for lower latency.
///
/// Implements `StreamCallback` to receive tool completion events. When a
/// read-only tool's arguments are complete, it spawns a tokio task to
/// execute it immediately. Write tools have their arguments pre-parsed
/// and stored for later use.
pub(super) struct StreamingToolExecutor {
    /// Queue of completed tool calls from stream events.
    completed_calls: Arc<Mutex<VecDeque<CompletedToolCall>>>,
    /// Handles to running early-execution tasks.
    running_tasks: Arc<Mutex<Vec<JoinHandle<EarlyToolResult>>>>,
    /// Completed early results, keyed by tool_call_id.
    finished_results: Arc<Mutex<HashMap<String, EarlyToolResult>>>,
    /// Pre-parsed args for write tools (keyed by tool_call_id).
    preparsed_write_args: Arc<Mutex<HashMap<String, PreparsedArgs>>>,
    /// Read-only tool names eligible for early execution.
    read_only_tools: HashSet<&'static str>,
    /// Tool registry for executing tools (shared ownership for spawned tasks).
    tool_registry: Arc<ToolRegistry>,
    /// Tool context for execution.
    tool_context: ToolContext,
    /// Cancellation token.
    cancel: Option<CancellationToken>,
    /// Semaphore to cap concurrent early executions.
    semaphore: Arc<tokio::sync::Semaphore>,
}

/// Maximum number of tools to execute concurrently during streaming.
const MAX_EARLY_CONCURRENT: usize = 4;

impl StreamingToolExecutor {
    /// Create a new streaming executor.
    pub fn new(
        tool_registry: Arc<ToolRegistry>,
        tool_context: ToolContext,
        cancel: Option<CancellationToken>,
    ) -> Self {
        Self {
            completed_calls: Arc::new(Mutex::new(VecDeque::new())),
            running_tasks: Arc::new(Mutex::new(Vec::new())),
            finished_results: Arc::new(Mutex::new(HashMap::new())),
            preparsed_write_args: Arc::new(Mutex::new(HashMap::new())),
            read_only_tools: opendev_tools_core::parallel::read_only_tools(),
            tool_registry,
            tool_context,
            cancel,
            semaphore: Arc::new(tokio::sync::Semaphore::new(MAX_EARLY_CONCURRENT)),
        }
    }

    /// Process a completed function call event from the stream.
    ///
    /// For read-only tools, spawns immediate execution. For write tools,
    /// pre-parses the arguments for later use.
    fn handle_function_done(&self, call_id: String, name: String, arguments: String) {
        if self.read_only_tools.contains(name.as_str()) {
            // Read-only tool: execute immediately
            debug!(
                tool = %name,
                call_id = %call_id,
                "Streaming executor: starting early execution of read-only tool"
            );

            let registry = Arc::clone(&self.tool_registry);
            let context = match &self.cancel {
                Some(ct) => {
                    let mut ctx = self.tool_context.clone();
                    ctx.cancel_token = Some(ct.child_token());
                    ctx
                }
                None => self.tool_context.clone(),
            };
            let sem = Arc::clone(&self.semaphore);
            let finished = Arc::clone(&self.finished_results);
            let tool_name = name.clone();
            let tc_id = call_id.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let start = std::time::Instant::now();

                // Parse arguments
                let args_value: Value =
                    serde_json::from_str(&arguments).unwrap_or(serde_json::json!({}));
                let args_map: HashMap<String, Value> = args_value
                    .as_object()
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();

                let wd_str = context.working_dir.to_string_lossy().to_string();
                let args_map = opendev_tools_core::normalizer::normalize_params(
                    &tool_name,
                    args_map,
                    Some(&wd_str),
                );

                let result = registry.execute(&tool_name, args_map, &context).await;
                let duration_ms = start.elapsed().as_millis() as u64;

                let early_result = EarlyToolResult {
                    call_id: tc_id.clone(),
                    tool_name: tool_name.clone(),
                    result,
                    duration_ms,
                };

                // Store in finished results
                if let Ok(mut map) = finished.lock() {
                    map.insert(
                        tc_id.clone(),
                        EarlyToolResult {
                            call_id: tc_id.clone(),
                            tool_name: tool_name.clone(),
                            result: opendev_tools_core::ToolResult {
                                success: early_result.result.success,
                                output: early_result.result.output.clone(),
                                error: early_result.result.error.clone(),
                                metadata: early_result.result.metadata.clone(),
                                duration_ms: Some(duration_ms),
                                llm_suffix: early_result.result.llm_suffix.clone(),
                            },
                            duration_ms,
                        },
                    );
                }

                early_result
            });

            if let Ok(mut tasks) = self.running_tasks.lock() {
                tasks.push(handle);
            }
        } else {
            // Write tool: pre-parse arguments for later use
            debug!(
                tool = %name,
                call_id = %call_id,
                "Streaming executor: pre-parsing write tool arguments"
            );

            let args_value: Value =
                serde_json::from_str(&arguments).unwrap_or(serde_json::json!({}));
            let args_map: HashMap<String, Value> = args_value
                .as_object()
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();

            let wd_str = self.tool_context.working_dir.to_string_lossy().to_string();
            let args_map =
                opendev_tools_core::normalizer::normalize_params(&name, args_map, Some(&wd_str));

            if let Ok(mut map) = self.preparsed_write_args.lock() {
                map.insert(call_id, PreparsedArgs { args_map });
            }
        }
    }

    /// Take an early result for a given tool_call_id, if available.
    ///
    /// This removes the result from the internal map. Returns `None` if the
    /// tool wasn't executed early or hasn't completed yet.
    pub fn take_result(&self, call_id: &str) -> Option<EarlyToolResult> {
        self.finished_results
            .lock()
            .ok()
            .and_then(|mut map| map.remove(call_id))
    }

    /// Take pre-parsed arguments for a write tool, if available.
    pub fn take_preparsed_args(&self, call_id: &str) -> Option<PreparsedArgs> {
        self.preparsed_write_args
            .lock()
            .ok()
            .and_then(|mut map| map.remove(call_id))
    }

    /// Wait for all running early tasks to complete.
    ///
    /// Should be called after streaming ends but before consuming results,
    /// to ensure all early-started tools have finished.
    pub async fn wait_for_completion(&self) {
        let handles: Vec<JoinHandle<EarlyToolResult>> = {
            let mut tasks = match self.running_tasks.lock() {
                Ok(t) => t,
                Err(_) => return,
            };
            std::mem::take(&mut *tasks)
        };

        for handle in handles {
            match handle.await {
                Ok(_) => {} // Result already stored in finished_results via the task
                Err(e) => {
                    warn!(error = %e, "Early tool execution task panicked");
                }
            }
        }
    }

    /// Returns true if any early results are available.
    #[allow(dead_code)]
    pub fn has_results(&self) -> bool {
        self.finished_results
            .lock()
            .ok()
            .is_some_and(|map| !map.is_empty())
    }

    /// Returns true if any tasks are still running.
    pub fn has_running_tasks(&self) -> bool {
        self.running_tasks
            .lock()
            .ok()
            .is_some_and(|tasks| !tasks.is_empty())
    }
}

impl StreamCallback for StreamingToolExecutor {
    fn on_event(&self, event: &StreamEvent) {
        // We track FunctionCallStart to build up the call_id → name mapping,
        // and FunctionCallDone to trigger execution.
        match event {
            StreamEvent::FunctionCallDone {
                index: _,
                arguments,
            } => {
                // FunctionCallDone doesn't carry call_id/name directly.
                // We need to get them from the completed_calls queue which
                // was populated by FunctionCallStart events.
                if let Ok(mut queue) = self.completed_calls.lock() {
                    // Match by order: FunctionCallStart always precedes FunctionCallDone
                    if let Some(call) = queue.pop_front() {
                        self.handle_function_done(call.call_id, call.name, arguments.clone());
                    }
                }
            }
            StreamEvent::FunctionCallStart {
                index: _,
                call_id,
                name,
            } => {
                // Queue the metadata for when FunctionCallDone arrives
                if let Ok(mut queue) = self.completed_calls.lock() {
                    queue.push_back(CompletedToolCall {
                        call_id: call_id.clone(),
                        name: name.clone(),
                        arguments: String::new(), // filled by FunctionCallDone
                    });
                }
            }
            _ => {} // Ignore text, reasoning, usage events
        }
    }
}

#[cfg(test)]
#[path = "streaming_executor_tests.rs"]
mod tests;
