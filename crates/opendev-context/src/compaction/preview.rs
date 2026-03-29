//! Compaction preview and tool output summarization.

use std::collections::HashSet;

use super::compactor::ContextCompactor;
use super::tokens::count_tokens;
use super::{
    ApiMessage, PROTECTED_TOOL_TYPES, PRUNE_PROTECTED_TOKENS, SLIDING_WINDOW_RECENT,
    SLIDING_WINDOW_THRESHOLD, TOOL_OUTPUT_SUMMARIZE_THRESHOLD,
};

/// Preview of what each compaction stage would remove.
#[derive(Debug, Clone, Default)]
pub struct CompactionPreview {
    /// Sliding window stage: messages that would be summarized.
    pub sliding_window: Option<StagePreview>,
    /// Mask stage: tool results that would be replaced with refs.
    pub mask: Option<StagePreview>,
    /// Summarize stage: verbose tool outputs that would be summarized.
    pub summarize: Option<StagePreview>,
    /// Prune stage: tool outputs that would be pruned.
    pub prune: Option<StagePreview>,
    /// Aggressive stage: additional masking.
    pub aggressive: Option<StagePreview>,
    /// Compact stage: middle messages that would be summarized.
    pub compact: Option<StagePreview>,
}

/// Stats for a single compaction stage.
#[derive(Debug, Clone)]
pub struct StagePreview {
    /// Number of messages affected.
    pub message_count: usize,
    /// Estimated token savings.
    pub estimated_token_savings: usize,
}

/// Returns a preview of what each compaction stage would remove, without
/// actually performing compaction.
pub fn compact_preview(messages: &[ApiMessage]) -> CompactionPreview {
    let mut preview = CompactionPreview::default();

    // --- Sliding window ---
    if messages.len() >= SLIDING_WINDOW_THRESHOLD {
        let summarized_count = messages.len().saturating_sub(SLIDING_WINDOW_RECENT + 1);
        if summarized_count > 0 {
            let tokens: usize = messages[1..=summarized_count]
                .iter()
                .map(msg_token_count)
                .sum();
            preview.sliding_window = Some(StagePreview {
                message_count: summarized_count,
                estimated_token_savings: tokens,
            });
        }
    }

    // --- Mask stage (level=Mask, recent_threshold=6) ---
    {
        let tool_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.get("role").and_then(|v| v.as_str()) == Some("tool"))
            .map(|(i, _)| i)
            .collect();
        let recent_threshold = 6;
        if tool_indices.len() > recent_threshold {
            let tc_map = ContextCompactor::build_tool_call_map(messages);
            let old_count = tool_indices.len() - recent_threshold;
            let mut maskable = 0usize;
            let mut token_savings = 0usize;
            for &i in &tool_indices[..old_count] {
                let content = messages[i]
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if content.starts_with("[ref:") {
                    continue;
                }
                let tool_call_id = messages[i]
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let tool_name = tc_map.get(tool_call_id).map(|s| s.as_str()).unwrap_or("");
                if PROTECTED_TOOL_TYPES.contains(&tool_name) {
                    continue;
                }
                maskable += 1;
                token_savings += count_tokens(content);
            }
            if maskable > 0 {
                preview.mask = Some(StagePreview {
                    message_count: maskable,
                    estimated_token_savings: token_savings,
                });
            }
        }
    }

    // --- Summarize stage (verbose tool outputs > 500 chars) ---
    {
        let mut summarizable = 0usize;
        let mut token_savings = 0usize;
        for msg in messages {
            if msg.get("role").and_then(|v| v.as_str()) != Some("tool") {
                continue;
            }
            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if content.len() > TOOL_OUTPUT_SUMMARIZE_THRESHOLD
                && !content.starts_with("[ref:")
                && content != "[pruned]"
                && !content.starts_with("[summary:")
            {
                summarizable += 1;
                let summary_tokens = count_tokens(&summarize_tool_output("tool", content));
                let original_tokens = count_tokens(content);
                token_savings += original_tokens.saturating_sub(summary_tokens);
            }
        }
        if summarizable > 0 {
            preview.summarize = Some(StagePreview {
                message_count: summarizable,
                estimated_token_savings: token_savings,
            });
        }
    }

    // --- Prune stage ---
    {
        let tc_map = ContextCompactor::build_tool_call_map(messages);
        let mut tool_indices: Vec<usize> = Vec::new();
        for i in (0..messages.len()).rev() {
            if messages[i].get("role").and_then(|v| v.as_str()) == Some("tool") {
                tool_indices.push(i);
            }
        }
        let mut protected_tokens: u64 = 0;
        let mut protected_indices: HashSet<usize> = HashSet::new();
        for &idx in &tool_indices {
            let content = messages[idx]
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if content.starts_with("[ref:")
                || content == "[pruned]"
                || content.starts_with("[summary:")
            {
                continue;
            }
            let tool_call_id = messages[idx]
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let tool_name = tc_map.get(tool_call_id).map(|s| s.as_str()).unwrap_or("");
            if PROTECTED_TOOL_TYPES.contains(&tool_name) {
                protected_indices.insert(idx);
                continue;
            }
            let token_estimate = content.len() as u64 / 4;
            if protected_tokens + token_estimate <= PRUNE_PROTECTED_TOKENS {
                protected_tokens += token_estimate;
                protected_indices.insert(idx);
            }
        }
        let mut prunable = 0usize;
        let mut token_savings = 0usize;
        for &idx in &tool_indices {
            if protected_indices.contains(&idx) {
                continue;
            }
            let content = messages[idx]
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if content.starts_with("[ref:")
                || content == "[pruned]"
                || content.starts_with("[summary:")
            {
                continue;
            }
            prunable += 1;
            token_savings += count_tokens(content);
        }
        if prunable > 0 {
            preview.prune = Some(StagePreview {
                message_count: prunable,
                estimated_token_savings: token_savings,
            });
        }
    }

    // --- Compact stage (would summarize middle messages) ---
    if messages.len() > 4 {
        let keep_recent = (messages.len() / 3).clamp(2, 5);
        let split_point = messages.len() - keep_recent;
        let middle_count = split_point.saturating_sub(1);
        if middle_count > 0 {
            let tokens: usize = messages[1..split_point].iter().map(msg_token_count).sum();
            preview.compact = Some(StagePreview {
                message_count: middle_count,
                estimated_token_savings: tokens,
            });
        }
    }

    preview
}

/// Estimate tokens for a single ApiMessage using the improved heuristic.
pub(super) fn msg_token_count(msg: &ApiMessage) -> usize {
    let content = msg.get("content");
    let mut total = 0usize;
    if let Some(serde_json::Value::Array(blocks)) = content {
        for block in blocks {
            if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                let text = block.get("text").and_then(|v| v.as_str()).unwrap_or("");
                total += count_tokens(text);
            }
        }
    } else if let Some(serde_json::Value::String(s)) = content {
        total += count_tokens(s);
    }
    if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in tool_calls {
            if let Some(func) = tc.get("function") {
                let name = func.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = func.get("arguments").and_then(|v| v.as_str()).unwrap_or("");
                total += count_tokens(name);
                total += count_tokens(args);
            }
        }
    }
    // Per-message overhead
    total += 4;
    total
}

/// Produce a 2-3 line summary of a verbose tool output.
///
/// Uses tool-specific logic: for `run_command` keeps exit code + last lines,
/// for `search`/`list_files` keeps result count + first results,
/// for other tools keeps first+last lines.
pub(super) fn summarize_tool_output(tool_name: &str, content: &str) -> String {
    use std::fmt::Write;

    let lines: Vec<&str> = content.lines().collect();
    let succeeded = !content.contains("error")
        && !content.contains("Error")
        && !content.contains("FAIL")
        && !content.contains("panic");
    let status = if succeeded { "succeeded" } else { "failed" };

    let mut buf = String::with_capacity(400);

    match tool_name {
        "run_command" | "bash" => {
            // For command outputs: keep exit code hint + last few lines
            let _ = write!(
                buf,
                "[summary: {tool_name} {status}, {} lines]",
                lines.len(),
            );
            // Show last 3 meaningful lines (often contain the result/error)
            let tail: Vec<&str> = lines
                .iter()
                .rev()
                .filter(|l| !l.trim().is_empty())
                .take(3)
                .copied()
                .collect();
            for line in tail.into_iter().rev() {
                let snippet: String = line.chars().take(150).collect();
                let _ = write!(buf, "\n{snippet}");
            }
        }
        "search" | "list_files" | "glob" | "grep" | "file_search" => {
            // For search results: keep count + first few results
            let result_count = lines.len();
            let _ = write!(
                buf,
                "[summary: {tool_name} {status}, {result_count} results]",
            );
            for line in lines.iter().take(5) {
                let snippet: String = line.chars().take(150).collect();
                let _ = write!(buf, "\n{snippet}");
            }
            if result_count > 5 {
                let _ = write!(buf, "\n... ({} more)", result_count - 5);
            }
        }
        _ => {
            // Generic: first + last lines
            let first_line = lines.first().map(|l| l.trim()).unwrap_or("");
            let last_line = if lines.len() > 1 {
                lines.last().map(|l| l.trim()).unwrap_or("")
            } else {
                ""
            };
            let first_snippet: String = first_line.chars().take(120).collect();
            let last_snippet: String = last_line.chars().take(120).collect();
            let _ = write!(
                buf,
                "[summary: {tool_name} {status}, {} lines]\n{first_snippet}",
                lines.len(),
            );
            if !last_snippet.is_empty() {
                let _ = write!(buf, "\n...\n{last_snippet}");
            }
        }
    }
    buf
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;
