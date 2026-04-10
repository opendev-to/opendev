use std::collections::HashMap;

use super::*;
use crate::traits::{BaseTool, ToolCategory, ToolContext, ToolResult};

fn tc(name: &str, args: serde_json::Value) -> ToolCall {
    ToolCall::new(name, args)
}

// --- Mock tools for partition_with_tools tests ---

#[derive(Debug)]
struct ReadOnlyTool;

#[async_trait::async_trait]
impl BaseTool for ReadOnlyTool {
    fn name(&self) -> &str {
        "ReadOnly"
    }
    fn description(&self) -> &str {
        "read-only tool"
    }
    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    fn is_read_only(&self, _args: &HashMap<String, serde_json::Value>) -> bool {
        true
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Read
    }
    async fn execute(
        &self,
        _args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        ToolResult::ok("ok")
    }
}

#[derive(Debug)]
struct WriteTool;

#[async_trait::async_trait]
impl BaseTool for WriteTool {
    fn name(&self) -> &str {
        "Write"
    }
    fn description(&self) -> &str {
        "write tool"
    }
    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Write
    }
    async fn execute(
        &self,
        _args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        ToolResult::ok("ok")
    }
}

#[derive(Debug)]
struct InputDependentTool;

#[async_trait::async_trait]
impl BaseTool for InputDependentTool {
    fn name(&self) -> &str {
        "Bash"
    }
    fn description(&self) -> &str {
        "input-dependent tool"
    }
    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    fn is_read_only(&self, args: &HashMap<String, serde_json::Value>) -> bool {
        args.get("command")
            .and_then(|v| v.as_str())
            .is_some_and(|cmd| cmd.starts_with("ls") || cmd.starts_with("cat"))
    }
    fn is_concurrent_safe(&self, args: &HashMap<String, serde_json::Value>) -> bool {
        self.is_read_only(args)
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Process
    }
    async fn execute(
        &self,
        _args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        ToolResult::ok("ok")
    }
}

#[test]
fn test_empty_partition() {
    let groups = ParallelPolicy::partition(&[]);
    assert!(groups.is_empty());
}

#[test]
fn test_single_tool() {
    let calls = vec![tc("Read", serde_json::json!({}))];
    let groups = ParallelPolicy::partition(&calls);
    assert_eq!(groups, vec![vec![0]]);
}

#[test]
fn test_all_read_only_single_batch() {
    let calls = vec![
        tc("Read", serde_json::json!({"file_path": "a.rs"})),
        tc("Grep", serde_json::json!({"query": "foo"})),
        tc("Glob", serde_json::json!({})),
    ];
    let groups = ParallelPolicy::partition(&calls);
    assert_eq!(groups, vec![vec![0, 1, 2]]);
}

#[test]
fn test_consecutive_batching_preserves_order() {
    // [Read, Grep, Edit, Read, Glob] → [[0,1], [2], [3,4]]
    let calls = vec![
        tc("Read", serde_json::json!({})),
        tc("Grep", serde_json::json!({})),
        tc("Edit", serde_json::json!({})),
        tc("Read", serde_json::json!({})),
        tc("Glob", serde_json::json!({})),
    ];
    let groups = ParallelPolicy::partition(&calls);
    assert_eq!(groups, vec![vec![0, 1], vec![2], vec![3, 4]]);
}

#[test]
fn test_all_writes_each_get_own_batch() {
    let calls = vec![
        tc("Bash", serde_json::json!({"command": "ls"})),
        tc("Write", serde_json::json!({"file_path": "a.rs"})),
        tc("Edit", serde_json::json!({"file_path": "b.rs"})),
    ];
    let groups = ParallelPolicy::partition(&calls);
    assert_eq!(groups, vec![vec![0], vec![1], vec![2]]);
}

#[test]
fn test_write_between_reads() {
    // Read, Write, Read → [[0], [1], [2]]
    let calls = vec![
        tc("Read", serde_json::json!({})),
        tc("Write", serde_json::json!({})),
        tc("Read", serde_json::json!({})),
    ];
    let groups = ParallelPolicy::partition(&calls);
    assert_eq!(groups, vec![vec![0], vec![1], vec![2]]);
}

#[test]
fn test_leading_write_then_reads() {
    // Edit, Read, Grep, Glob → [[0], [1,2,3]]
    let calls = vec![
        tc("Edit", serde_json::json!({})),
        tc("Read", serde_json::json!({})),
        tc("Grep", serde_json::json!({})),
        tc("Glob", serde_json::json!({})),
    ];
    let groups = ParallelPolicy::partition(&calls);
    assert_eq!(groups, vec![vec![0], vec![1, 2, 3]]);
}

#[test]
fn test_mcp_tools_are_non_concurrent() {
    let calls = vec![
        tc("Read", serde_json::json!({})),
        tc("mcp__github__create_issue", serde_json::json!({})),
        tc("Grep", serde_json::json!({})),
    ];
    let groups = ParallelPolicy::partition(&calls);
    assert_eq!(groups, vec![vec![0], vec![1], vec![2]]);
}

#[test]
fn test_web_tools_are_concurrent() {
    let calls = vec![
        tc("WebFetch", serde_json::json!({})),
        tc("WebSearch", serde_json::json!({})),
        tc("Read", serde_json::json!({})),
    ];
    let groups = ParallelPolicy::partition(&calls);
    assert_eq!(groups, vec![vec![0, 1, 2]]);
}

#[test]
fn test_has_parallel_batches() {
    assert!(!ParallelPolicy::has_parallel_batches(&[]));
    assert!(!ParallelPolicy::has_parallel_batches(&[vec![0]]));
    assert!(!ParallelPolicy::has_parallel_batches(&[vec![0], vec![1]]));
    assert!(ParallelPolicy::has_parallel_batches(&[vec![0, 1]]));
    assert!(ParallelPolicy::has_parallel_batches(&[vec![0], vec![1, 2]]));
}

// -----------------------------------------------------------------------
// partition_with_tools — trait-based partitioning
// -----------------------------------------------------------------------

#[test]
fn test_partition_with_tools_empty() {
    let groups = ParallelPolicy::partition_with_tools(&[], &[]);
    assert!(groups.is_empty());
}

#[test]
fn test_partition_with_tools_single() {
    let ro = ReadOnlyTool;
    let calls = vec![tc("ReadOnly", serde_json::json!({}))];
    let tools: Vec<&dyn BaseTool> = vec![&ro];
    let groups = ParallelPolicy::partition_with_tools(&calls, &tools);
    assert_eq!(groups, vec![vec![0]]);
}

#[test]
fn test_partition_with_tools_all_read_only() {
    let ro1 = ReadOnlyTool;
    let ro2 = ReadOnlyTool;
    let ro3 = ReadOnlyTool;
    let calls = vec![
        tc("ReadOnly", serde_json::json!({})),
        tc("ReadOnly", serde_json::json!({})),
        tc("ReadOnly", serde_json::json!({})),
    ];
    let tools: Vec<&dyn BaseTool> = vec![&ro1, &ro2, &ro3];
    let groups = ParallelPolicy::partition_with_tools(&calls, &tools);
    assert_eq!(groups, vec![vec![0, 1, 2]]);
}

#[test]
fn test_partition_with_tools_all_writes() {
    let w1 = WriteTool;
    let w2 = WriteTool;
    let calls = vec![
        tc("Write", serde_json::json!({})),
        tc("Write", serde_json::json!({})),
    ];
    let tools: Vec<&dyn BaseTool> = vec![&w1, &w2];
    let groups = ParallelPolicy::partition_with_tools(&calls, &tools);
    assert_eq!(groups, vec![vec![0], vec![1]]);
}

#[test]
fn test_partition_with_tools_mixed_preserves_order() {
    let ro1 = ReadOnlyTool;
    let ro2 = ReadOnlyTool;
    let w = WriteTool;
    let ro3 = ReadOnlyTool;
    let ro4 = ReadOnlyTool;
    let calls = vec![
        tc("ReadOnly", serde_json::json!({})),
        tc("ReadOnly", serde_json::json!({})),
        tc("Write", serde_json::json!({})),
        tc("ReadOnly", serde_json::json!({})),
        tc("ReadOnly", serde_json::json!({})),
    ];
    let tools: Vec<&dyn BaseTool> = vec![&ro1, &ro2, &w, &ro3, &ro4];
    let groups = ParallelPolicy::partition_with_tools(&calls, &tools);
    assert_eq!(groups, vec![vec![0, 1], vec![2], vec![3, 4]]);
}

#[test]
fn test_partition_with_tools_input_dependent_bash() {
    let bash_ls = InputDependentTool;
    let bash_rm = InputDependentTool;
    let bash_cat = InputDependentTool;
    let calls = vec![
        tc("Bash", serde_json::json!({"command": "ls -la"})),
        tc("Bash", serde_json::json!({"command": "rm file.txt"})),
        tc("Bash", serde_json::json!({"command": "cat foo.rs"})),
    ];
    let tools: Vec<&dyn BaseTool> = vec![&bash_ls, &bash_rm, &bash_cat];
    let groups = ParallelPolicy::partition_with_tools(&calls, &tools);
    assert_eq!(groups, vec![vec![0], vec![1], vec![2]]);
}

#[test]
fn test_partition_with_tools_bash_reads_batched_with_reads() {
    let ro = ReadOnlyTool;
    let bash_ls = InputDependentTool;
    let ro2 = ReadOnlyTool;
    let calls = vec![
        tc("Read", serde_json::json!({})),
        tc("Bash", serde_json::json!({"command": "ls -la"})),
        tc("Read", serde_json::json!({})),
    ];
    let tools: Vec<&dyn BaseTool> = vec![&ro, &bash_ls, &ro2];
    let groups = ParallelPolicy::partition_with_tools(&calls, &tools);
    assert_eq!(groups, vec![vec![0, 1, 2]]);
}

#[test]
fn test_partition_with_tools_bash_write_breaks_batch() {
    let ro = ReadOnlyTool;
    let bash_rm = InputDependentTool;
    let ro2 = ReadOnlyTool;
    let calls = vec![
        tc("Read", serde_json::json!({})),
        tc("Bash", serde_json::json!({"command": "rm file.txt"})),
        tc("Read", serde_json::json!({})),
    ];
    let tools: Vec<&dyn BaseTool> = vec![&ro, &bash_rm, &ro2];
    let groups = ParallelPolicy::partition_with_tools(&calls, &tools);
    assert_eq!(groups, vec![vec![0], vec![1], vec![2]]);
}
