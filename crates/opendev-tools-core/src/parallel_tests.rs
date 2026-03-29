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
