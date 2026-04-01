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
