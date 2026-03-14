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
        // Git (read-only actions checked separately)
        "git",
        // Meta (read-only)
        "list_todos",
        "search_tools",
        "task_complete",
        // Agents listing
        "list_agents",
    ])
}

/// Git actions that are safe to parallelize.
fn read_only_git_actions() -> HashSet<&'static str> {
    HashSet::from(["status", "diff", "log", "branch"])
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
        if !ro_tools.contains(tc.name.as_str()) {
            return false;
        }

        // Special case: git depends on action
        if tc.name == "git" {
            let ro_git = read_only_git_actions();
            let action = tc
                .arguments
                .get("action")
                .and_then(|a| a.as_str())
                .unwrap_or("");
            return ro_git.contains(action);
        }

        true
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
mod tests {
    use super::*;

    fn tc(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall::new(name, args)
    }

    #[test]
    fn test_empty_partition() {
        let groups = ParallelPolicy::partition(&[]);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_single_tool() {
        let calls = vec![tc("read_file", serde_json::json!({}))];
        let groups = ParallelPolicy::partition(&calls);
        assert_eq!(groups, vec![vec![0]]);
    }

    #[test]
    fn test_all_read_only_parallel() {
        let calls = vec![
            tc("read_file", serde_json::json!({"file_path": "a.rs"})),
            tc("search", serde_json::json!({"query": "foo"})),
            tc("list_files", serde_json::json!({})),
        ];
        let groups = ParallelPolicy::partition(&calls);
        // All reads in one group
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
    }

    #[test]
    fn test_mixed_read_write() {
        let calls = vec![
            tc("read_file", serde_json::json!({"file_path": "a.rs"})),
            tc("write_file", serde_json::json!({"file_path": "b.rs"})),
            tc("read_file", serde_json::json!({"file_path": "c.rs"})),
        ];
        let groups = ParallelPolicy::partition(&calls);
        // Group 1: reads [0, 2], Group 2: write [1]
        assert_eq!(groups.len(), 2);
        assert!(groups[0].contains(&0));
        assert!(groups[0].contains(&2));
        assert_eq!(groups[1], vec![1]);
    }

    #[test]
    fn test_writes_different_files_parallel() {
        let calls = vec![
            tc("write_file", serde_json::json!({"file_path": "a.rs"})),
            tc("write_file", serde_json::json!({"file_path": "b.rs"})),
        ];
        let groups = ParallelPolicy::partition(&calls);
        // Different files -> one parallel group
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_writes_same_file_sequential() {
        let calls = vec![
            tc("write_file", serde_json::json!({"file_path": "a.rs"})),
            tc("write_file", serde_json::json!({"file_path": "a.rs"})),
        ];
        let groups = ParallelPolicy::partition(&calls);
        // Same file -> sequential (separate groups)
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_git_read_only_action() {
        let calls = vec![
            tc("git", serde_json::json!({"action": "status"})),
            tc("git", serde_json::json!({"action": "diff"})),
        ];
        let groups = ParallelPolicy::partition(&calls);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_git_write_action() {
        let calls = vec![
            tc("git", serde_json::json!({"action": "commit"})),
            tc("read_file", serde_json::json!({})),
        ];
        let groups = ParallelPolicy::partition(&calls);
        // read_file is read-only, git commit is "other" (not in write_tools)
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_run_command_sequential() {
        let calls = vec![
            tc("run_command", serde_json::json!({"command": "ls"})),
            tc("run_command", serde_json::json!({"command": "pwd"})),
        ];
        let groups = ParallelPolicy::partition(&calls);
        // run_command is a write tool, no file_path -> sequential
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_mcp_tools_are_other() {
        let calls = vec![
            tc("mcp__github__create_issue", serde_json::json!({})),
            tc("read_file", serde_json::json!({})),
        ];
        let groups = ParallelPolicy::partition(&calls);
        // read in group 1, mcp in group 2
        assert_eq!(groups.len(), 2);
    }
}
