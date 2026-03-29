//! Execute tools and format results.
//!
//! Mirrors `opendev/repl/tool_executor.py`.

use std::collections::HashMap;
use std::time::Instant;

use futures::future::join_all;
use serde_json::Value;
use tracing::{debug, info, warn};

use opendev_tools_core::parallel::{ParallelPolicy, ToolCall as PolicyToolCall};
use opendev_tools_core::{ToolContext, ToolRegistry, ToolResult};

use crate::error::ReplError;

/// Result of a tool execution with timing metadata.
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    /// Tool name that was executed.
    pub tool_name: String,
    /// Whether the tool succeeded.
    pub success: bool,
    /// Tool output (on success).
    pub output: Option<String>,
    /// Error message (on failure).
    pub error: Option<String>,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}

/// Handles tool execution with approval, undo, and result formatting.
pub struct ToolExecutor {
    /// Number of tools executed in this session.
    execution_count: u64,
}

impl ToolExecutor {
    /// Create a new tool executor.
    pub fn new() -> Self {
        Self { execution_count: 0 }
    }

    /// Execute a single tool call.
    ///
    /// Parses the tool call JSON, dispatches via the registry,
    /// and returns a formatted result.
    pub async fn execute(
        &mut self,
        tool_call: &Value,
        registry: &ToolRegistry,
        context: &ToolContext,
    ) -> Result<ToolExecutionResult, ReplError> {
        let tool_name = tool_call["function"]["name"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        self.execution_count += 1;
        info!(
            tool = %tool_name,
            execution_num = self.execution_count,
            "Executing tool"
        );

        let start = Instant::now();

        // Parse arguments to HashMap<String, Value>
        let args_value = &tool_call["function"]["arguments"];
        let args: HashMap<String, Value> = if let Some(args_str) = args_value.as_str() {
            serde_json::from_str(args_str).unwrap_or_default()
        } else if let Some(obj) = args_value.as_object() {
            obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            HashMap::new()
        };

        // Execute via registry (returns ToolResult directly)
        let result: ToolResult = registry.execute(&tool_name, args, context).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        if result.success {
            debug!(
                tool = %tool_name,
                duration_ms,
                "Tool execution succeeded"
            );
        } else {
            warn!(
                tool = %tool_name,
                error = ?result.error,
                duration_ms,
                "Tool execution failed"
            );
        }

        Ok(ToolExecutionResult {
            tool_name,
            success: result.success,
            output: result.output,
            error: result.error,
            duration_ms,
        })
    }

    /// Execute multiple tool calls, potentially in parallel for read-only tools.
    ///
    /// Uses [`ParallelPolicy`] to partition tool calls into execution groups.
    /// Read-only tools within the same group run concurrently via `join_all`.
    /// Groups are executed in order to preserve write-after-read semantics.
    pub async fn execute_batch(
        &mut self,
        tool_calls: &[Value],
        registry: &ToolRegistry,
        context: &ToolContext,
    ) -> Vec<Result<ToolExecutionResult, ReplError>> {
        if tool_calls.is_empty() {
            return vec![];
        }

        // Build PolicyToolCall descriptors for partitioning.
        let policy_calls: Vec<PolicyToolCall> = tool_calls
            .iter()
            .map(|tc| {
                let name = tc["function"]["name"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let arguments = tc["function"]["arguments"].clone();
                let args_val = if let Some(s) = arguments.as_str() {
                    serde_json::from_str(s).unwrap_or(Value::Object(Default::default()))
                } else {
                    arguments
                };
                PolicyToolCall::new(name, args_val)
            })
            .collect();

        let groups = ParallelPolicy::partition(&policy_calls);

        // Pre-allocate result slots (filled out-of-order for parallel groups).
        let mut results: Vec<Option<Result<ToolExecutionResult, ReplError>>> =
            (0..tool_calls.len()).map(|_| None).collect();

        for group in &groups {
            if group.len() == 1 {
                // Single tool -- run sequentially (avoids spawn overhead).
                let idx = group[0];
                self.execution_count += 1;
                let res = self
                    .execute_single(&tool_calls[idx], registry, context)
                    .await;
                results[idx] = Some(res);
            } else {
                // Multiple tools in this group -- run concurrently.
                let futs: Vec<_> = group
                    .iter()
                    .map(|&idx| {
                        let tc = &tool_calls[idx];
                        Self::execute_standalone(tc, registry, context)
                    })
                    .collect();

                let group_results = join_all(futs).await;
                for (&idx, res) in group.iter().zip(group_results) {
                    self.execution_count += 1;
                    results[idx] = Some(res);
                }
            }
        }

        // Unwrap Option wrappers (all slots should be filled).
        results
            .into_iter()
            .enumerate()
            .map(|(i, opt)| {
                opt.unwrap_or_else(|| {
                    Ok(ToolExecutionResult {
                        tool_name: format!("tool_{}", i),
                        success: false,
                        output: None,
                        error: Some("tool was not scheduled for execution".to_string()),
                        duration_ms: 0,
                    })
                })
            })
            .collect()
    }

    /// Execute a single tool call (updates internal execution_count).
    async fn execute_single(
        &mut self,
        tool_call: &Value,
        registry: &ToolRegistry,
        context: &ToolContext,
    ) -> Result<ToolExecutionResult, ReplError> {
        self.execute(tool_call, registry, context).await
    }

    /// Execute a tool call without borrowing &mut self (for parallel use).
    async fn execute_standalone(
        tool_call: &Value,
        registry: &ToolRegistry,
        context: &ToolContext,
    ) -> Result<ToolExecutionResult, ReplError> {
        let tool_name = tool_call["function"]["name"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let start = Instant::now();

        let args_value = &tool_call["function"]["arguments"];
        let args: HashMap<String, Value> = if let Some(args_str) = args_value.as_str() {
            serde_json::from_str(args_str).unwrap_or_default()
        } else if let Some(obj) = args_value.as_object() {
            obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            HashMap::new()
        };

        let result: ToolResult = registry.execute(&tool_name, args, context).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        if result.success {
            debug!(tool = %tool_name, duration_ms, "Tool execution succeeded (parallel)");
        } else {
            warn!(tool = %tool_name, error = ?result.error, duration_ms, "Tool execution failed (parallel)");
        }

        Ok(ToolExecutionResult {
            tool_name,
            success: result.success,
            output: result.output,
            error: result.error,
            duration_ms,
        })
    }

    /// Format a tool execution result for display.
    pub fn format_result(result: &ToolExecutionResult) -> String {
        if result.success {
            format!(
                "  {} ({}ms)\n{}",
                result.tool_name,
                result.duration_ms,
                result.output.as_deref().unwrap_or("")
            )
        } else {
            format!(
                "  {} FAILED ({}ms)\n  Error: {}",
                result.tool_name,
                result.duration_ms,
                result.error.as_deref().unwrap_or("unknown error")
            )
        }
    }
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tool_executor_tests.rs"]
mod tests;
