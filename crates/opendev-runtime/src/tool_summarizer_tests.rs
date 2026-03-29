use super::*;

#[test]
fn test_error_summary() {
    let summary = summarize_tool_result("read_file", None, Some("file not found"));
    assert_eq!(summary, "Error: file not found");
}

#[test]
fn test_error_truncation() {
    let long_error = "x".repeat(300);
    let summary = summarize_tool_result("read_file", None, Some(&long_error));
    assert!(summary.len() <= 210); // "Error: " + 200 chars
}

#[test]
fn test_empty_output() {
    let summary = summarize_tool_result("read_file", Some(""), None);
    assert_eq!(summary, "Success (no output)");
}

#[test]
fn test_no_output() {
    let summary = summarize_tool_result("write_file", None, None);
    assert_eq!(summary, "Success (no output)");
}

#[test]
fn test_read_file() {
    let output = "line1\nline2\nline3";
    let summary = summarize_tool_result("read_file", Some(output), None);
    assert_eq!(summary, "Read file (3 lines, 17 chars)");
}

#[test]
fn test_write_file() {
    let summary = summarize_tool_result("write_file", Some("wrote 100 bytes"), None);
    assert_eq!(summary, "File written successfully");
}

#[test]
fn test_edit_file() {
    let summary = summarize_tool_result("edit_file", Some("patched"), None);
    assert_eq!(summary, "File edited successfully");
}

#[test]
fn test_search_no_matches() {
    let summary = summarize_tool_result("search", Some("No matches found"), None);
    assert_eq!(summary, "Search completed (0 matches)");
}

#[test]
fn test_search_with_matches() {
    let output = "src/main.rs:10: fn main()\nsrc/lib.rs:5: pub mod config\nsrc/app.rs:1: use std";
    let summary = summarize_tool_result("search", Some(output), None);
    assert_eq!(summary, "Search completed (3 matches found)");
}

#[test]
fn test_list_files() {
    let output = "file1.rs\nfile2.rs\nfile3.rs";
    let summary = summarize_tool_result("list_files", Some(output), None);
    assert_eq!(summary, "Listed directory (3 items)");
}

#[test]
fn test_bash_short_output() {
    let summary = summarize_tool_result("run_command", Some("hello world"), None);
    assert_eq!(summary, "Output: hello world");
}

#[test]
fn test_bash_long_output() {
    let output = (0..20)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let summary = summarize_tool_result("run_command", Some(&output), None);
    assert_eq!(summary, "Command executed (20 lines of output)");
}

#[test]
fn test_web_fetch() {
    let summary = summarize_tool_result("web_fetch", Some("<html>...</html>"), None);
    assert_eq!(summary, "Content fetched successfully");
}

#[test]
fn test_git_short() {
    let summary = summarize_tool_result("git", Some("Already up to date."), None);
    assert_eq!(summary, "Output: Already up to date.");
}

#[test]
fn test_generic_short() {
    let summary = summarize_tool_result("unknown_tool", Some("done"), None);
    assert_eq!(summary, "done");
}

#[test]
fn test_generic_long() {
    let output = "x".repeat(200);
    let summary = summarize_tool_result("unknown_tool", Some(&output), None);
    assert!(summary.contains("Success"));
    assert!(summary.contains("200 chars"));
}

// --- build_background_result tests ---

#[test]
fn test_background_result_rich_content_used_as_is() {
    // Content must be >= 500 chars to skip subagent appendix
    let content = format!(
        "Here is a detailed summary of my findings across the codebase. {}",
        "The architecture uses 21 crates. ".repeat(20)
    );
    assert!(content.len() >= 500, "test content must be >= 500 chars");
    let content = &content;
    let messages = vec![
        serde_json::json!({"role": "tool", "name": "spawn_subagent", "content": "raw output"}),
    ];
    let result = build_background_result(content, &messages, 12000);
    // Content is > 500 chars, so subagent outputs should NOT be appended
    assert!(!result.contains("## Subagent Outputs"));
    assert!(result.starts_with("Here is a detailed"));
}

#[test]
fn test_background_result_thin_content_appends_subagents() {
    let content = "Done.";
    let messages = vec![
        serde_json::json!({"role": "user", "content": "explore"}),
        serde_json::json!({"role": "tool", "name": "spawn_subagent", "content": "Found 21 crates in the workspace."}),
        serde_json::json!({"role": "tool", "name": "spawn_subagent", "content": "Tests use tempfile for isolation."}),
        serde_json::json!({"role": "tool", "name": "read_file", "content": "some file content"}),
    ];
    let result = build_background_result(content, &messages, 12000);
    assert!(result.starts_with("Done."));
    assert!(result.contains("## Subagent Outputs"));
    assert!(result.contains("Found 21 crates"));
    assert!(result.contains("Tests use tempfile"));
    // read_file should NOT appear (not a spawn_subagent)
    assert!(!result.contains("some file content"));
}

#[test]
fn test_background_result_no_messages() {
    let result = build_background_result("All done.", &[], 12000);
    assert_eq!(result, "All done.");
}

#[test]
fn test_background_result_no_subagent_tools() {
    let content = "Ok.";
    let messages =
        vec![serde_json::json!({"role": "tool", "name": "read_file", "content": "data"})];
    let result = build_background_result(content, &messages, 12000);
    // Thin content but no spawn_subagent results → no appendix
    assert_eq!(result, "Ok.");
}

#[test]
fn test_background_result_budget_enforcement() {
    let content = "Short.";
    let big_output = "x".repeat(20000);
    let messages =
        vec![serde_json::json!({"role": "tool", "name": "spawn_subagent", "content": big_output})];
    let result = build_background_result(content, &messages, 5000);
    assert!(result.len() <= 5100); // 5000 + "... [truncated]" suffix
    assert!(result.contains("... [truncated]"));
}

#[test]
fn test_background_result_content_truncation() {
    let content = "a".repeat(20000);
    let result = build_background_result(&content, &[], 12000);
    // 2/3 of 12000 = 8000 content cap
    assert!(result.len() <= 8100);
    assert!(result.contains("... [truncated]"));
}

#[test]
fn test_safe_truncate_ascii() {
    assert_eq!(safe_truncate("hello world", 5), "hello");
    assert_eq!(safe_truncate("hi", 10), "hi");
}

#[test]
fn test_safe_truncate_multibyte() {
    // "é" is 2 bytes in UTF-8
    let s = "café";
    // "caf" = 3 bytes, "é" = bytes 3-4
    // Truncating at 4 should include the é
    assert_eq!(safe_truncate(s, 5), "café");
    // Truncating at 3 should NOT split the é
    assert_eq!(safe_truncate(s, 4), "caf");
}
