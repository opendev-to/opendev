//! End-to-end tests for `tool_budget` against a real temp directory.
//!
//! Verifies that overflow files land where the displayed reference path
//! claims they do, and that consecutive calls produce distinct files.

use opendev_context::{OverflowStore, ToolBudgetPolicy, apply_tool_result_budget};

#[test]
fn overflow_path_resolves_relative_to_project_root() {
    let tmp = tempfile::tempdir().unwrap();
    let store = OverflowStore::new(tmp.path());
    let policy = ToolBudgetPolicy::with_default_chars(64);

    let raw = "data ".repeat(200);
    let result = apply_tool_result_budget("custom", "call-A", &raw, &policy, &store);
    let rel_path = result.overflow_ref.expect("must overflow");

    let abs = tmp.path().join(&rel_path);
    assert!(
        abs.exists(),
        "overflow file should exist at {}",
        abs.display()
    );
    let on_disk = std::fs::read_to_string(&abs).unwrap();
    assert_eq!(on_disk, raw);
}

#[test]
fn distinct_calls_produce_distinct_overflow_files() {
    let tmp = tempfile::tempdir().unwrap();
    let store = OverflowStore::new(tmp.path());
    let policy = ToolBudgetPolicy::with_default_chars(32);

    let raw = "x".repeat(500);
    let r1 = apply_tool_result_budget("t", "id-1", &raw, &policy, &store);
    let r2 = apply_tool_result_budget("t", "id-2", &raw, &policy, &store);

    let p1 = r1.overflow_ref.unwrap();
    let p2 = r2.overflow_ref.unwrap();
    assert_ne!(p1, p2, "tool_call_id must differentiate filenames");
}

/// I1: Mirror the agent loop's call site — `apply_tool_result_budget`
/// is called between formatting the raw tool result and pushing it
/// into the `messages` list. The pushed message MUST carry the
/// budgeted (truncated) content, never the raw payload. This is the
/// invariant the entire feature rests on.
#[test]
fn pushed_tool_message_carries_budgeted_content_not_raw() {
    let tmp = tempfile::tempdir().unwrap();
    let store = OverflowStore::new(tmp.path());
    let policy = ToolBudgetPolicy::with_default_chars(64);

    let raw = "RAW_SHOULD_NEVER_APPEAR_IN_FULL ".repeat(100);
    let budgeted = apply_tool_result_budget("custom_tool", "call-X", &raw, &policy, &store);

    // Mirror the exact push pattern used in
    // crates/opendev-agents/src/react_loop/phases/tool_dispatch.rs.
    let mut messages: Vec<serde_json::Value> = Vec::new();
    messages.push(serde_json::json!({
        "role": "tool",
        "tool_call_id": "call-X",
        "name": "custom_tool",
        "content": budgeted.displayed_content,
    }));

    let pushed = messages[0]["content"]
        .as_str()
        .expect("content must be a string");
    let pushed_chars = pushed.chars().count();

    assert_eq!(pushed, budgeted.displayed_content);
    assert!(
        pushed_chars < raw.chars().count(),
        "pushed content must be smaller than raw — pushed {} vs raw {}",
        pushed_chars,
        raw.chars().count(),
    );
    // Truncation marker is present in the pushed message.
    assert!(pushed.contains("[truncated:"));
    assert!(pushed.contains("[full output:"));
    // And the raw payload's repeated marker substring does NOT appear in
    // full — at most as a prefix within the preview.
    let raw_marker_count = pushed.matches("RAW_SHOULD_NEVER_APPEAR_IN_FULL").count();
    assert!(
        raw_marker_count < 100,
        "pushed content seems to contain the full raw payload",
    );
}

#[test]
fn write_failure_degrades_gracefully() {
    // Point the overflow dir at a path that cannot be created (under a
    // file rather than a directory). The displayed content must still
    // be bounded with a truncation marker, just without a reference.
    let tmp = tempfile::tempdir().unwrap();
    let blocking_file = tmp.path().join("blocker");
    std::fs::write(&blocking_file, "not a directory").unwrap();

    let store = OverflowStore::with_dir(tmp.path(), blocking_file.join("nested"));
    let policy = ToolBudgetPolicy::with_default_chars(16);

    let raw = "y".repeat(200);
    let result = apply_tool_result_budget("custom", "id", &raw, &policy, &store);

    assert!(result.truncated);
    assert!(result.overflow_ref.is_none(), "must degrade without panic");
    assert!(result.displayed_content.contains("[truncated:"));
    assert!(!result.displayed_content.contains("[full output:"));
}
