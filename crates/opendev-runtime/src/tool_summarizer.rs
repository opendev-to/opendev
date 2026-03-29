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
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
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
    if result.len() > total_budget {
        let end = safe_truncate(&result, total_budget).len();
        result.truncate(end);
        result.push_str("... [truncated]");
    }
    result
}

#[cfg(test)]
#[path = "tool_summarizer_tests.rs"]
mod tests;
