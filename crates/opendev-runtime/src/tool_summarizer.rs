//! Tool result summarizer — creates concise summaries for LLM context.
//!
//! Mirrors Python's `opendev/core/utils/tool_result_summarizer.py`.
//! Summaries are stored in `ToolCall.result_summary` to prevent context
//! bloat while preserving semantic meaning for the LLM.

/// Create a concise 1-2 line summary of a tool result for LLM context.
///
/// This prevents context bloat while maintaining semantic meaning.
/// Summaries are typically 50-200 chars.
pub fn summarize_tool_result(tool_name: &str, output: Option<&str>, error: Option<&str>) -> String {
    // Error case
    if let Some(err) = error {
        let truncated = if err.len() > 200 { &err[..200] } else { err };
        return format!("Error: {truncated}");
    }

    let result_str = output.unwrap_or("");

    if result_str.is_empty() {
        return "Success (no output)".to_string();
    }

    match tool_name {
        // File read operations
        "read_file" | "Read" => {
            let lines = result_str.lines().count();
            let chars = result_str.len();
            format!("Read file ({lines} lines, {chars} chars)")
        }

        // File write operations
        "write_file" | "Write" => "File written successfully".to_string(),

        // File edit operations
        "edit_file" | "Edit" => "File edited successfully".to_string(),

        // Delete operations
        "delete_file" | "Delete" => "File deleted".to_string(),

        // Search operations
        "search" | "Grep" | "file_search" => {
            if result_str.contains("No matches found") || result_str.trim().is_empty() {
                "Search completed (0 matches)".to_string()
            } else {
                let match_count = result_str.lines().count();
                format!("Search completed ({match_count} matches found)")
            }
        }

        // Directory listing
        "list_files" | "list_directory" | "List" => {
            let file_count = if result_str.is_empty() {
                0
            } else {
                result_str.lines().count()
            };
            format!("Listed directory ({file_count} items)")
        }

        // Command execution
        "run_command" | "Run" | "bash_execute" | "Bash" => {
            let lines = result_str.lines().count();
            if lines > 10 {
                format!("Command executed ({lines} lines of output)")
            } else if result_str.len() < 100 {
                format!("Output: {}", &result_str[..result_str.len().min(100)])
            } else {
                "Command executed successfully".to_string()
            }
        }

        // Web operations
        "fetch_url" | "Fetch" | "web_fetch" | "web_search" => {
            "Content fetched successfully".to_string()
        }

        // Screenshot operations
        "capture_screenshot" | "web_screenshot" | "analyze_image" => {
            "Image processed successfully".to_string()
        }

        // Git operations
        "git" => {
            let lines = result_str.lines().count();
            if lines > 10 {
                format!("Git operation completed ({lines} lines)")
            } else if result_str.len() < 100 {
                format!("Output: {}", &result_str[..result_str.len().min(100)])
            } else {
                "Git operation completed".to_string()
            }
        }

        // Todo tools
        "write_todos" => {
            let count = result_str
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    t.starts_with("[todo]") || t.starts_with("[doing]") || t.starts_with("[done]")
                })
                .count();
            if count == 1 {
                "Created 1 todo".to_string()
            } else if count > 1 {
                format!("Created {count} todos")
            } else {
                "Todos updated".to_string()
            }
        }
        "update_todo" => "Todo updated".to_string(),
        "complete_todo" => "Todo completed".to_string(),
        "list_todos" => {
            let count = result_str
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    t.starts_with("[todo]") || t.starts_with("[doing]") || t.starts_with("[done]")
                })
                .count();
            format!("{count} todos listed")
        }
        "clear_todos" => "All todos cleared".to_string(),

        // Generic fallback
        _ => {
            if result_str.len() < 100 {
                result_str.to_string()
            } else {
                let chars = result_str.len();
                let lines = result_str.lines().count();
                format!("Success ({lines} lines, {chars} chars)")
            }
        }
    }
}

/// Truncate a string at a char boundary, never in the middle of a multi-byte char.
fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Walk back from max_bytes to find a char boundary
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Build a rich result string from a background agent's completion.
///
/// Uses the LLM's own summary (`content`) as primary content. When the summary
/// is too short (< 500 chars), appends raw `spawn_subagent` tool outputs as a
/// reference appendix so the foreground agent has the actual data.
pub fn build_background_result(
    content: &str,
    messages: &[serde_json::Value],
    total_budget: usize,
) -> String {
    let mut result = String::new();

    // 1. Always include agent's own summary (cap at 2/3 of budget)
    let content_cap = total_budget * 2 / 3;
    let trimmed = safe_truncate(content, content_cap);
    result.push_str(trimmed);
    if content.len() > content_cap {
        result.push_str("... [truncated]");
    }

    // 2. If summary is thin, append raw subagent outputs
    if content.len() < 500 {
        let subagent_outputs: Vec<&str> = messages
            .iter()
            .filter_map(|m| {
                let role = m.get("role")?.as_str()?;
                let name = m.get("name")?.as_str()?;
                let text = m.get("content")?.as_str()?;
                if role == "tool" && name == "spawn_subagent" && !text.is_empty() {
                    Some(text)
                } else {
                    None
                }
            })
            .collect();

        if !subagent_outputs.is_empty() {
            let remaining = total_budget.saturating_sub(result.len() + 100);
            let per_agent = remaining / subagent_outputs.len();

            result.push_str("\n\n## Subagent Outputs\n");
            for (i, output) in subagent_outputs.iter().enumerate() {
                result.push_str(&format!("\n### Subagent {}\n", i + 1));
                let trimmed = safe_truncate(output, per_agent);
                result.push_str(trimmed);
                if output.len() > per_agent {
                    result.push_str("... [truncated]");
                }
                result.push('\n');
            }
        }
    }

    // 3. Final safety cap
    let final_trimmed = safe_truncate(&result, total_budget);
    if final_trimmed.len() < result.len() {
        let mut out = final_trimmed.to_string();
        out.push_str("... [truncated]");
        out
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
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
        let output =
            "src/main.rs:10: fn main()\nsrc/lib.rs:5: pub mod config\nsrc/app.rs:1: use std";
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
        let messages = vec![serde_json::json!({"role": "tool", "name": "spawn_subagent", "content": "raw output"})];
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
        let messages = vec![
            serde_json::json!({"role": "tool", "name": "read_file", "content": "data"}),
        ];
        let result = build_background_result(content, &messages, 12000);
        // Thin content but no spawn_subagent results → no appendix
        assert_eq!(result, "Ok.");
    }

    #[test]
    fn test_background_result_budget_enforcement() {
        let content = "Short.";
        let big_output = "x".repeat(20000);
        let messages = vec![
            serde_json::json!({"role": "tool", "name": "spawn_subagent", "content": big_output}),
        ];
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
}
