//! Auto-compaction of conversation history when approaching context limits.
//!
//! Implements staged context optimization with proactive reduction:
//! - Sliding window: For 500+ message sessions, keep recent N + compressed summary
//! - 70%: Warning logged, tracking begins
//! - 80%: Progressive observation masking + verbose tool output summarization
//! - 85%: Fast pruning of old tool outputs (skips small outputs < 200 chars)
//! - 90%: Aggressive masking + trimming
//! - 99%: Full LLM-powered compaction (summarize middle messages)

mod artifacts;
mod compactor;
mod levels;
mod preview;
mod tokens;

pub use artifacts::{ArtifactEntry, ArtifactIndex};
pub use compactor::ContextCompactor;
pub use levels::OptimizationLevel;
pub use preview::{CompactionPreview, StagePreview, compact_preview};
pub use tokens::count_tokens;

/// Staged compaction thresholds (fraction of context window).
pub const STAGE_WARNING: f64 = 0.70;
pub const STAGE_MASK: f64 = 0.80;
pub const STAGE_PRUNE: f64 = 0.85;
pub const STAGE_AGGRESSIVE: f64 = 0.90;
pub const STAGE_COMPACT: f64 = 0.99;

/// Token budget to protect from pruning (recent tool outputs).
pub const PRUNE_PROTECTED_TOKENS: u64 = 40_000;

/// Tool types whose outputs survive compaction pruning.
pub const PROTECTED_TOOL_TYPES: &[&str] = &[
    "skill",
    "invoke_skill",
    "present_plan",
    "read_file",
    "web_screenshot",
    "vlm",
];

/// Minimum output length below which pruning is skipped (not worth it).
pub const PRUNE_MIN_LENGTH: usize = 200;

/// Sliding window: number of recent messages to keep verbatim.
pub const SLIDING_WINDOW_RECENT: usize = 50;

/// Sliding window: message count threshold to activate.
pub const SLIDING_WINDOW_THRESHOLD: usize = 500;

/// Minimum length of tool output before summarization kicks in.
pub const TOOL_OUTPUT_SUMMARIZE_THRESHOLD: usize = 500;

/// Default per-tool result character budget. Outputs above this are
/// truncated to a preview and overflow content is persisted to disk.
/// Roughly ~2K tokens, sized to keep a single tool result from
/// dominating the typical 80K-token context window.
pub const TOOL_RESULT_BUDGET_DEFAULT_CHARS: usize = 8_000;

/// Number of leading characters retained in the displayed preview when
/// a tool result exceeds its budget. Smaller than the cap so the
/// truncation marker and reference path always fit comfortably.
pub const TOOL_RESULT_BUDGET_PREVIEW_CHARS: usize = 1_500;

/// Subdirectory under the project's `.opendev/` directory where overflow
/// tool result content is stored.
pub const TOOL_RESULT_BUDGET_OVERFLOW_DIR: &str = "tool-results";

/// A message in API format (role + content + optional tool_calls).
///
/// This is a lightweight representation for compaction operations,
/// working with raw JSON-like dicts rather than the full ChatMessage model.
pub type ApiMessage = serde_json::Map<String, serde_json::Value>;

/// Test helpers shared across sub-module tests.
#[cfg(test)]
pub(crate) mod tests {
    use super::ApiMessage;

    pub fn make_msg(role: &str, content: &str) -> ApiMessage {
        let mut msg = ApiMessage::new();
        msg.insert(
            "role".to_string(),
            serde_json::Value::String(role.to_string()),
        );
        msg.insert(
            "content".to_string(),
            serde_json::Value::String(content.to_string()),
        );
        msg
    }

    pub fn make_tool_msg(tool_call_id: &str, content: &str) -> ApiMessage {
        let mut msg = ApiMessage::new();
        msg.insert(
            "role".to_string(),
            serde_json::Value::String("tool".to_string()),
        );
        msg.insert(
            "tool_call_id".to_string(),
            serde_json::Value::String(tool_call_id.to_string()),
        );
        msg.insert(
            "content".to_string(),
            serde_json::Value::String(content.to_string()),
        );
        msg
    }

    pub fn make_assistant_with_tc(tool_calls: Vec<(&str, &str)>) -> ApiMessage {
        let mut msg = ApiMessage::new();
        msg.insert(
            "role".to_string(),
            serde_json::Value::String("assistant".to_string()),
        );
        msg.insert(
            "content".to_string(),
            serde_json::Value::String(String::new()),
        );
        let tcs: Vec<serde_json::Value> = tool_calls
            .into_iter()
            .map(|(id, name)| {
                serde_json::json!({
                    "id": id,
                    "function": { "name": name, "arguments": "{}" }
                })
            })
            .collect();
        msg.insert("tool_calls".to_string(), serde_json::Value::Array(tcs));
        msg
    }
}
