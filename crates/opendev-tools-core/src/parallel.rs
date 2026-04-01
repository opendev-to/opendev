//! Parallel execution policy for tool calls.
//!
//! Determines which tool calls can be safely executed in parallel and partitions
//! them into ordered execution groups.

use std::collections::HashSet;

/// Tools that are always safe to parallelize (read-only operations).
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
/// Consecutive read-only tools are grouped into a single batch (safe to run in
/// parallel). Non-read-only tools each get their own batch (run serially).
/// Batches execute in strict positional order, preserving the LLM's intended
/// tool call sequence.
///
/// Example: `[Read, Grep, Edit, Read, Glob]` → `[[0,1], [2], [3,4]]`
pub struct ParallelPolicy;

impl ParallelPolicy {
    /// Partition tool calls into ordered execution batches.
    ///
    /// Each batch can be executed in parallel internally. Batches must execute
    /// in order. Batches with a single element run serially.
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
