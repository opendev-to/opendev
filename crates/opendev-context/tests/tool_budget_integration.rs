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
