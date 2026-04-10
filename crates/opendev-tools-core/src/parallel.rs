//! Parallel execution policy for tool calls.
//!
//! Determines which tool calls can be safely executed in parallel and partitions
//! them into ordered execution groups. Uses `BaseTool::is_concurrent_safe()` to
//! make input-dependent decisions (e.g., `Bash` with `ls` is safe, `Bash` with
//! `rm` is not).

use std::collections::HashMap;
use std::collections::HashSet;

use crate::traits::BaseTool;

/// Tools that are always safe to parallelize (read-only operations).
///
/// **Deprecated:** This hardcoded list is kept as a fallback for the legacy
/// `partition()` method. New callers should use `partition_with_tools()` which
/// consults `BaseTool::is_concurrent_safe()` instead.
fn read_only_tools() -> HashSet<&'static str> {
    HashSet::from([
        // File reading
        "Read",
        "Glob",
        "Grep",
        "find_symbol",
        "find_referencing_symbols",
        "analyze_image",
        // Web (read-only)
        "WebFetch",
        "WebSearch",
        "capture_web_screenshot",
        "capture_screenshot",
        // Session/memory (read-only)
        "list_sessions",
        "get_session_history",
        "list_subagents",
        "memory_search",
        // Meta (read-only)
        "TaskList",
        "search_tools",
        "TaskStop",
        // Agents listing
        "list_agents",
    ])
}

/// A tool call with its name and arguments, used for partitioning.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

impl ToolCall {
    pub fn new(name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            arguments,
        }
    }
}

/// Partitions tool calls into ordered execution batches.
///
/// Consecutive concurrent-safe tools are grouped into a single batch (safe to
/// run in parallel). Non-concurrent tools each get their own batch (run
/// serially). Batches execute in strict positional order, preserving the LLM's
/// intended tool call sequence.
///
/// Example: `[Read, Grep, Edit, Read, Glob]` → `[[0,1], [2], [3,4]]`
pub struct ParallelPolicy;

impl ParallelPolicy {
    /// Partition tool calls using `BaseTool::is_concurrent_safe()`.
    ///
    /// This is the preferred method — it consults each tool's trait method,
    /// enabling input-dependent concurrency decisions.
    pub fn partition_with_tools(
        tool_calls: &[ToolCall],
        tools: &[&dyn BaseTool],
    ) -> Vec<Vec<usize>> {
        if tool_calls.len() <= 1 {
            return if tool_calls.is_empty() {
                vec![]
            } else {
                vec![vec![0]]
            };
        }

        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut current_concurrent: Vec<usize> = Vec::new();

        for (i, tc) in tool_calls.iter().enumerate() {
            let args: HashMap<String, serde_json::Value> =
                if let Some(obj) = tc.arguments.as_object() {
                    obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                } else {
                    HashMap::new()
                };

            let is_safe = tools
                .get(i)
                .map(|t| t.is_concurrent_safe(&args))
                .unwrap_or(false);

            if is_safe {
                current_concurrent.push(i);
            } else {
                if !current_concurrent.is_empty() {
                    groups.push(std::mem::take(&mut current_concurrent));
                }
                groups.push(vec![i]);
            }
        }

        if !current_concurrent.is_empty() {
            groups.push(current_concurrent);
        }

        groups
    }

    /// Partition tool calls using the hardcoded read-only tool list.
    ///
    /// **Deprecated:** Prefer `partition_with_tools()` which uses trait methods.
    pub fn partition(tool_calls: &[ToolCall]) -> Vec<Vec<usize>> {
        if tool_calls.len() <= 1 {
            return if tool_calls.is_empty() {
                vec![]
            } else {
                vec![vec![0]]
            };
        }

        let ro_tools = read_only_tools();
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut current_concurrent: Vec<usize> = Vec::new();

        for (i, tc) in tool_calls.iter().enumerate() {
            if ro_tools.contains(tc.name.as_str()) {
                current_concurrent.push(i);
            } else {
                // Flush accumulated concurrent batch
                if !current_concurrent.is_empty() {
                    groups.push(std::mem::take(&mut current_concurrent));
                }
                // Non-concurrent tool gets its own batch
                groups.push(vec![i]);
            }
        }

        // Flush trailing concurrent batch
        if !current_concurrent.is_empty() {
            groups.push(current_concurrent);
        }

        groups
    }

    /// Returns true if any batch has more than one element (parallelism possible).
    pub fn has_parallel_batches(batches: &[Vec<usize>]) -> bool {
        batches.iter().any(|b| b.len() > 1)
    }
}

#[cfg(test)]
#[path = "parallel_tests.rs"]
mod tests;
