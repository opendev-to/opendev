use super::super::tests::{make_assistant_with_tc, make_msg, make_tool_msg};
use super::*;

#[test]
fn test_compact_preview_small() {
    let messages = vec![
        make_msg("system", "sys"),
        make_msg("user", "hi"),
        make_msg("assistant", "hello"),
    ];
    let preview = compact_preview(&messages);
    // No sliding window (too few messages)
    assert!(preview.sliding_window.is_none());
    // No mask (no tool messages)
    assert!(preview.mask.is_none());
}

#[test]
fn test_compact_preview_with_tool_messages() {
    let mut messages = vec![make_msg("system", "sys")];
    let tc_ids: Vec<String> = (0..10).map(|i| format!("tc-{i}")).collect();
    let pairs: Vec<(&str, &str)> = tc_ids.iter().map(|id| (id.as_str(), "bash")).collect();
    messages.push(make_assistant_with_tc(pairs));
    for id in &tc_ids {
        messages.push(make_tool_msg(id, &"output data ".repeat(100)));
    }

    let preview = compact_preview(&messages);
    // Should have mask preview (10 tool msgs, keep 6 recent -> 4 maskable)
    assert!(preview.mask.is_some());
    let mask = preview.mask.unwrap();
    assert_eq!(mask.message_count, 4);
    assert!(mask.estimated_token_savings > 0);

    // Should have summarize preview (each output > 500 chars)
    assert!(preview.summarize.is_some());
    let summarize = preview.summarize.unwrap();
    assert_eq!(summarize.message_count, 10);
}

#[test]
fn test_compact_preview_compact_stage() {
    let mut messages = vec![make_msg("system", "sys")];
    for i in 0..20 {
        messages.push(make_msg("user", &format!("question {i}")));
        messages.push(make_msg("assistant", &format!("answer {i}")));
    }
    let preview = compact_preview(&messages);
    assert!(preview.compact.is_some());
    let compact = preview.compact.unwrap();
    assert!(compact.message_count > 0);
    assert!(compact.estimated_token_savings > 0);
}

#[test]
fn test_summarize_tool_output_success() {
    let output = format!(
        "Line 1: all good\n{}\nLine 100: done",
        "some data\n".repeat(50)
    );
    let summary = summarize_tool_output("bash", &output);
    assert!(summary.starts_with("[summary: bash succeeded"));
    // bash/run_command branch keeps last 3 meaningful lines
    assert!(summary.contains("Line 100: done"));
}

#[test]
fn test_summarize_tool_output_failure() {
    let output = "Error: file not found\nbacktrace follows\npanic at line 42";
    let summary = summarize_tool_output("bash", output);
    assert!(summary.contains("failed"));
}

#[test]
fn test_summarize_tool_output_generic() {
    let output = format!(
        "First line of output\n{}\nLast line of output",
        "middle data\n".repeat(50)
    );
    let summary = summarize_tool_output("edit_file", &output);
    assert!(summary.starts_with("[summary: edit_file succeeded"));
    // Generic branch keeps first + last lines
    assert!(summary.contains("First line of output"));
    assert!(summary.contains("Last line of output"));
}

#[test]
fn test_summarize_run_command_keeps_tail() {
    let output = format!(
        "Compiling opendev v0.1.0\n{}\n    Finished release [optimized] target(s) in 42.3s",
        "   Compiling some-crate v1.0.0\n".repeat(20)
    );
    let summary = summarize_tool_output("run_command", &output);
    assert!(summary.starts_with("[summary: run_command succeeded"));
    // Should contain the last line (build result)
    assert!(summary.contains("Finished release"));
}

#[test]
fn test_summarize_search_keeps_first_results() {
    let output = (0..20)
        .map(|i| format!("src/file_{i}.rs:42: match found"))
        .collect::<Vec<_>>()
        .join("\n");
    let summary = summarize_tool_output("search", &output);
    assert!(summary.starts_with("[summary: search succeeded, 20 results]"));
    // Should show first 5 results
    assert!(summary.contains("src/file_0.rs"));
    assert!(summary.contains("src/file_4.rs"));
    // Should indicate more
    assert!(summary.contains("15 more"));
}

#[test]
fn test_msg_token_count_uses_heuristic() {
    let msg = make_msg("user", "The quick brown fox jumps over the lazy dog.");
    let tokens = msg_token_count(&msg);
    // Should be > 0 and use the heuristic (not just chars/4)
    assert!(tokens > 0);
    // The per-message overhead is 4 tokens
    assert!(tokens >= 4);
}
