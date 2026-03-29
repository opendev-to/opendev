//! Parallel execution policy for tool calls.
//!
//! Determines which tool calls can be safely executed in parallel and partitions
//! them into ordered execution groups.

use std::collections::HashSet;

/// Tools that are always safe to parallelize (read-only operations).
fn read_only_tools() -> HashSet<&'static str> {
    HashSet::from([
        // File reading
        "read_file",
        "list_files",
        "search",
        "find_symbol",
        "find_referencing_symbols",
        "analyze_image",
        // Web (read-only)
        "fetch_url",
        "web_search",
        "capture_web_screenshot",
        "capture_screenshot",
        // Session/memory (read-only)
        "list_sessions",
        "get_session_history",
        "list_subagents",
        "memory_search",
        // Meta (read-only)
        "list_todos",
        "search_tools",
        "task_complete",
        // Agents listing
        "list_agents",
    ])
}

/// Tools that modify state and should generally run sequentially.
fn write_tools() -> HashSet<&'static str> {
    HashSet::from([
        "write_file",
        "edit_file",
        "multi_edit",
        "run_command",
        "insert_before_symbol",
        "insert_after_symbol",
        "replace_symbol_body",
        "rename_symbol",
        "notebook_edit",
        "apply_patch",
        "memory_write",
        "write_todos",
        "update_todo",
        "complete_todo",
        "clear_todos",
        "send_message",
        "schedule",
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

/// Partitions tool calls into execution groups for optimal parallelism.
///
/// Rules:
/// 1. All read-only tools can run in parallel (group 1)
/// 2. Write tools targeting different files can parallelize (group 2)
/// 3. Everything else runs sequentially (individual groups)
pub struct ParallelPolicy;

impl ParallelPolicy {
    /// Partition tool calls into ordered execution groups.
    ///
    /// Each group can be executed in parallel. Groups must be executed in order.
    pub fn partition(tool_calls: &[ToolCall]) -> Vec<Vec<usize>> {
        if tool_calls.len() <= 1 {
            return if tool_calls.is_empty() {
                vec![]
            } else {
                vec![vec![0]]
            };
        }

        let ro_tools = read_only_tools();
        let w_tools = write_tools();

        let mut read_indices: Vec<usize> = Vec::new();
        let mut write_indices: Vec<usize> = Vec::new();
        let mut other_indices: Vec<usize> = Vec::new();

        for (i, tc) in tool_calls.iter().enumerate() {
            if Self::is_read_only(tc, &ro_tools) {
                read_indices.push(i);
            } else if w_tools.contains(tc.name.as_str()) {
                write_indices.push(i);
            } else {
                other_indices.push(i);
            }
        }

        let mut groups: Vec<Vec<usize>> = Vec::new();

        // Group 1: All read-only tools (parallel)
        if !read_indices.is_empty() {
            groups.push(read_indices);
        }

        // Group 2: Write tools
        if !write_indices.is_empty() {
            if Self::can_parallelize_writes(tool_calls, &write_indices) {
                groups.push(write_indices);
            } else {
                for idx in write_indices {
                    groups.push(vec![idx]);
                }
            }
        }

        // Group 3: Everything else (sequential)
        for idx in other_indices {
            groups.push(vec![idx]);
        }

        groups
    }

    /// Check if a tool call is read-only.
    fn is_read_only(tc: &ToolCall, ro_tools: &HashSet<&str>) -> bool {
        ro_tools.contains(tc.name.as_str())
    }

    /// Check if write operations target different files (safe to parallelize).
    fn can_parallelize_writes(tool_calls: &[ToolCall], write_indices: &[usize]) -> bool {
        let mut targets: HashSet<String> = HashSet::new();

        for &idx in write_indices {
            let tc = &tool_calls[idx];
            match tc.name.as_str() {
                "write_file" | "edit_file" | "notebook_edit" => {
                    let target = tc
                        .arguments
                        .get("file_path")
                        .or_else(|| tc.arguments.get("notebook_path"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if target.is_empty() {
                        return false;
                    }
                    if targets.contains(target) {
                        return false; // Same file -> sequential
                    }
                    targets.insert(target.to_string());
                }
                _ => return false, // Non-file writes -> sequential
            }
        }

        targets.len() > 1
    }
}

#[cfg(test)]
#[path = "parallel_tests.rs"]
mod tests;
