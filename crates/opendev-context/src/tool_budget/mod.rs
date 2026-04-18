//! Preemptive per-tool result budgeting.
//!
//! Complements the staged compaction in [`crate::compaction`]: where
//! compaction reacts after total context usage crosses thresholds,
//! budgeting caps each individual tool result at write-time so a
//! single oversized output cannot push the conversation from 60% to
//! 95% in one turn.
//!
//! Outputs above their per-tool cap are truncated to a short preview
//! and the full content is persisted via [`OverflowStore`]. The
//! displayed message ends with a reference path the agent can re-read
//! on demand.

mod overflow;
mod policy;

pub use overflow::OverflowStore;
pub use policy::ToolBudgetPolicy;

use crate::compaction::TOOL_RESULT_BUDGET_PREVIEW_CHARS;

/// Result of applying [`apply_tool_result_budget`] to a tool output.
#[derive(Debug, Clone)]
pub struct BudgetedResult {
    /// Content to embed in the tool message sent to the model. Always
    /// `<= cap` characters; equals `raw_output` when no truncation
    /// was needed.
    pub displayed_content: String,
    /// Display-form reference path written by the [`OverflowStore`]
    /// when truncation occurred and persistence succeeded. `None`
    /// means the content fit within budget OR the overflow write
    /// failed (in which case `displayed_content` carries a
    /// truncation marker without a reference).
    pub overflow_ref: Option<String>,
    /// Length of the original (pre-truncation) output in characters.
    pub original_len: usize,
    /// Whether the output was truncated.
    pub truncated: bool,
}

/// Apply the per-tool budget to `raw_output`. Pure transformation —
/// the only side effect is the optional overflow file write performed
/// by `overflow_store`. A failed write degrades gracefully: the
/// returned `displayed_content` is still bounded, just without a
/// reference path.
///
/// `tool_call_id` is used to make overflow filenames unique and to
/// correlate the on-disk content with the in-conversation tool call.
pub fn apply_tool_result_budget(
    tool_name: &str,
    tool_call_id: &str,
    raw_output: &str,
    policy: &ToolBudgetPolicy,
    overflow_store: &OverflowStore,
) -> BudgetedResult {
    let original_len = raw_output.chars().count();
    let cap = policy.cap_for(tool_name);

    if cap == usize::MAX || char_len_within(raw_output, cap) {
        return BudgetedResult {
            displayed_content: raw_output.to_string(),
            overflow_ref: None,
            original_len,
            truncated: false,
        };
    }

    let preview_len = TOOL_RESULT_BUDGET_PREVIEW_CHARS.min(cap);
    let preview = take_chars(raw_output, preview_len);
    let overflow_ref = overflow_store.write(tool_name, tool_call_id, raw_output);

    let displayed_content = match &overflow_ref {
        Some(path) => format!(
            "{preview}\n\n…\n[truncated: {omitted} / {total} chars omitted]\n[full output: {path}]",
            omitted = original_len.saturating_sub(preview_len),
            total = original_len,
        ),
        None => format!(
            "{preview}\n\n…\n[truncated: {omitted} / {total} chars omitted]",
            omitted = original_len.saturating_sub(preview_len),
            total = original_len,
        ),
    };

    BudgetedResult {
        displayed_content,
        overflow_ref,
        original_len,
        truncated: true,
    }
}

/// Counts chars without materializing — bails as soon as the count
/// exceeds `limit` so we do not walk a 1MB output to learn it is too big.
fn char_len_within(s: &str, limit: usize) -> bool {
    s.chars().take(limit + 1).count() <= limit
}

/// Take the first `n` characters of `s` as an owned `String`. Char-aware
/// so multi-byte boundaries are never split.
fn take_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
