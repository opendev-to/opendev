//! Auto-compaction of conversation history when approaching context limits.
//!
//! Implements staged context optimization with proactive reduction:
//! - Sliding window: For 500+ message sessions, keep recent N + compressed summary
//! - 70%: Warning logged, tracking begins
//! - 80%: Progressive observation masking (old tool results -> compact refs)
//! - 85%: Smart tool output summarization, then fast pruning of old tool outputs
//! - 90%: Aggressive masking + trimming
//! - 99%: Full LLM-powered compaction (summarize middle messages)

use std::collections::{HashMap, HashSet};

use chrono::Local;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Staged compaction thresholds (fraction of context window).
pub const STAGE_WARNING: f64 = 0.70;
pub const STAGE_MASK: f64 = 0.80;
pub const STAGE_PRUNE: f64 = 0.85;
pub const STAGE_AGGRESSIVE: f64 = 0.90;
pub const STAGE_COMPACT: f64 = 0.99;

/// Token budget to protect from pruning (recent tool outputs).
pub const PRUNE_PROTECTED_TOKENS: u64 = 40_000;

/// Tool types whose outputs survive compaction pruning.
pub const PROTECTED_TOOL_TYPES: &[&str] = &["skill", "present_plan", "read_file"];

/// Sliding window: number of recent messages to keep verbatim.
pub const SLIDING_WINDOW_RECENT: usize = 50;

/// Sliding window: message count threshold to activate.
pub const SLIDING_WINDOW_THRESHOLD: usize = 500;

/// Minimum length of tool output before summarization kicks in.
pub const TOOL_OUTPUT_SUMMARIZE_THRESHOLD: usize = 500;

/// Count tokens in text using a cl100k_base-style heuristic.
///
/// Splits on whitespace and punctuation boundaries and applies a ~0.75
/// tokens-per-word ratio, which is more accurate than the naive `chars / 4`
/// approximation for English prose and code.
pub fn count_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    // Split on whitespace first
    let word_count: usize = text
        .split_whitespace()
        .map(|word| {
            // Each word may contain punctuation that the tokenizer splits off.
            // Count extra tokens for punctuation sequences attached to words.
            let punct_count = word
                .chars()
                .filter(|c| c.is_ascii_punctuation())
                .count();
            // Base: 1 token per word fragment, plus extra tokens for
            // punctuation clusters (each punctuation char is ~0.5 token on
            // average, but we round up since BPE often keeps single-char
            // punctuation as its own token).
            1 + (punct_count + 1) / 2
        })
        .sum();
    // Apply the 0.75 ratio: most English words map to < 1 BPE token.
    // We use integer math: (count * 3 + 2) / 4 ≈ ceil(count * 0.75).
    (word_count * 3 + 2) / 4
}

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
                .map(|m| msg_token_count(m))
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
            if content.starts_with("[ref:") || content == "[pruned]" || content.starts_with("[summary:") {
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
            if content.starts_with("[ref:") || content == "[pruned]" || content.starts_with("[summary:") {
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
            let tokens: usize = messages[1..split_point]
                .iter()
                .map(|m| msg_token_count(m))
                .sum();
            preview.compact = Some(StagePreview {
                message_count: middle_count,
                estimated_token_savings: tokens,
            });
        }
    }

    preview
}

/// Estimate tokens for a single ApiMessage using the improved heuristic.
fn msg_token_count(msg: &ApiMessage) -> usize {
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
/// Keeps the tool name, success/failure indication, and first+last lines.
fn summarize_tool_output(tool_name: &str, content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let succeeded = !content.contains("error")
        && !content.contains("Error")
        && !content.contains("FAIL")
        && !content.contains("panic");
    let status = if succeeded { "succeeded" } else { "failed" };

    let first_line = lines.first().map(|l| l.trim()).unwrap_or("");
    let last_line = if lines.len() > 1 {
        lines.last().map(|l| l.trim()).unwrap_or("")
    } else {
        ""
    };

    let first_snippet: String = first_line.chars().take(120).collect();
    let last_snippet: String = last_line.chars().take(120).collect();

    if last_snippet.is_empty() {
        format!(
            "[summary: {tool_name} {status}, {line_count} lines]\n{first_snippet}",
            line_count = lines.len(),
        )
    } else {
        format!(
            "[summary: {tool_name} {status}, {line_count} lines]\n{first_snippet}\n...\n{last_snippet}",
            line_count = lines.len(),
        )
    }
}

/// Optimization level returned by `check_usage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationLevel {
    /// No optimization needed.
    None,
    /// 70%: Warning logged, tracking begins.
    Warning,
    /// 80%: Progressive observation masking.
    Mask,
    /// 85%: Fast pruning of old tool outputs.
    Prune,
    /// 90%: Aggressive masking + trimming.
    Aggressive,
    /// 99%: Full LLM-powered compaction.
    Compact,
}

impl OptimizationLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Warning => "warning",
            Self::Mask => "mask",
            Self::Prune => "prune",
            Self::Aggressive => "aggressive",
            Self::Compact => "compact",
        }
    }
}

/// Tracks files touched during a session, surviving compaction.
///
/// Records file operations (create, modify, read, delete) with metadata
/// so the agent retains awareness of workspace state post-compaction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtifactIndex {
    entries: HashMap<String, ArtifactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub file_path: String,
    pub last_operation: String,
    pub last_details: String,
    pub created_at: String,
    pub updated_at: String,
    pub operation_count: u32,
    pub operations_seen: Vec<String>,
}

impl ArtifactIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a file operation.
    pub fn record(&mut self, file_path: &str, operation: &str, details: &str) {
        let now = Local::now().to_rfc3339();
        if let Some(existing) = self.entries.get_mut(file_path) {
            existing.last_operation = operation.to_string();
            existing.last_details = details.to_string();
            existing.updated_at = now;
            existing.operation_count += 1;
            if !existing.operations_seen.contains(&operation.to_string()) {
                existing.operations_seen.push(operation.to_string());
            }
        } else {
            self.entries.insert(
                file_path.to_string(),
                ArtifactEntry {
                    file_path: file_path.to_string(),
                    last_operation: operation.to_string(),
                    last_details: details.to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                    operation_count: 1,
                    operations_seen: vec![operation.to_string()],
                },
            );
        }
    }

    /// Format the artifact index as a compact summary for injection into compaction.
    pub fn as_summary(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let mut lines = vec!["## Artifact Index (files touched this session)".to_string()];
        for (path, entry) in &self.entries {
            let ops = entry.operations_seen.join(", ");
            let detail = if entry.last_details.is_empty() {
                String::new()
            } else {
                format!(" — {}", entry.last_details)
            };
            lines.push(format!("- `{path}` [{ops}]{detail}"));
        }
        lines.join("\n")
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialize the artifact index to a JSON value for session persistence.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// Deserialize an artifact index from a JSON value (loaded from session metadata).
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }
}

/// A message in API format (role + content + optional tool_calls).
///
/// This is a lightweight representation for compaction operations,
/// working with raw JSON-like dicts rather than the full ChatMessage model.
pub type ApiMessage = serde_json::Map<String, serde_json::Value>;

/// Auto-compacts conversation history when approaching context limits.
pub struct ContextCompactor {
    max_context: u64,
    last_token_count: u64,
    api_prompt_tokens: u64,
    msg_count_at_calibration: usize,
    warned_70: bool,
    warned_80: bool,
    warned_90: bool,
    session_id: Option<String>,
    pub artifact_index: ArtifactIndex,
}

impl ContextCompactor {
    pub fn new(max_context_tokens: u64) -> Self {
        info!(
            "ContextCompactor: max_context={} tokens",
            max_context_tokens
        );
        Self {
            max_context: max_context_tokens,
            last_token_count: 0,
            api_prompt_tokens: 0,
            msg_count_at_calibration: 0,
            warned_70: false,
            warned_80: false,
            warned_90: false,
            session_id: None,
            artifact_index: ArtifactIndex::new(),
        }
    }

    pub fn set_session_id(&mut self, session_id: String) {
        self.session_id = Some(session_id);
    }

    /// Save the artifact index into a session metadata map.
    ///
    /// Stores under the key `"artifact_index"` so it persists across
    /// session save/load cycles.
    pub fn save_artifact_index(
        &self,
        metadata: &mut std::collections::HashMap<String, serde_json::Value>,
    ) {
        if !self.artifact_index.is_empty() {
            metadata.insert("artifact_index".to_string(), self.artifact_index.to_json());
        }
    }

    /// Restore the artifact index from session metadata.
    ///
    /// Looks for the `"artifact_index"` key and deserializes it.
    pub fn load_artifact_index(
        &mut self,
        metadata: &std::collections::HashMap<String, serde_json::Value>,
    ) {
        if let Some(value) = metadata.get("artifact_index")
            && let Some(index) = ArtifactIndex::from_json(value)
        {
            info!(
                "Restored artifact index with {} entries from session",
                index.len()
            );
            self.artifact_index = index;
        }
    }

    /// Context usage as percentage (0-100+).
    pub fn usage_pct(&self) -> f64 {
        if self.max_context == 0 || self.last_token_count == 0 {
            return 0.0;
        }
        (self.last_token_count as f64 / self.max_context as f64) * 100.0
    }

    /// Percentage points remaining before full compaction triggers.
    pub fn pct_until_compact(&self) -> f64 {
        let threshold_pct = STAGE_COMPACT * 100.0;
        (threshold_pct - self.usage_pct()).max(0.0)
    }

    /// Check context usage and return the appropriate optimization level.
    pub fn check_usage(
        &mut self,
        messages: &[ApiMessage],
        system_prompt: &str,
    ) -> OptimizationLevel {
        self.update_token_count(messages, system_prompt);
        let pct = self.usage_pct() / 100.0;

        if pct >= STAGE_COMPACT {
            return OptimizationLevel::Compact;
        }
        if pct >= STAGE_AGGRESSIVE {
            if !self.warned_90 {
                warn!(
                    "Context at {:.1}% — aggressive optimization active",
                    pct * 100.0
                );
                self.warned_90 = true;
            }
            return OptimizationLevel::Aggressive;
        }
        if pct >= STAGE_PRUNE {
            return OptimizationLevel::Prune;
        }
        if pct >= STAGE_MASK {
            if !self.warned_80 {
                warn!(
                    "Context at {:.1}% — observation masking active",
                    pct * 100.0
                );
                self.warned_80 = true;
            }
            return OptimizationLevel::Mask;
        }
        if pct >= STAGE_WARNING {
            if !self.warned_70 {
                info!("Context at {:.1}% — approaching limits", pct * 100.0);
                self.warned_70 = true;
            }
            return OptimizationLevel::Warning;
        }
        OptimizationLevel::None
    }

    /// Check if conversation exceeds the compaction threshold.
    pub fn should_compact(&mut self, messages: &[ApiMessage], system_prompt: &str) -> bool {
        self.update_token_count(messages, system_prompt);
        self.last_token_count > (self.max_context as f64 * STAGE_COMPACT) as u64
    }

    /// Calibrate with real API token count.
    pub fn update_from_api_usage(&mut self, prompt_tokens: u64, message_count: usize) {
        if prompt_tokens > 0 {
            self.api_prompt_tokens = prompt_tokens;
            self.msg_count_at_calibration = message_count;
            self.last_token_count = prompt_tokens;
        } else {
            debug!(
                "update_from_api_usage: prompt_tokens=0, skipping calibration \
                 (max_context={}, last_token_count={})",
                self.max_context, self.last_token_count,
            );
        }
    }

    /// Build a mapping from tool_call_id to tool function name.
    pub fn build_tool_call_map(messages: &[ApiMessage]) -> HashMap<String, String> {
        let mut tc_map = HashMap::new();
        for msg in messages {
            if msg.get("role").and_then(|v| v.as_str()) != Some("assistant") {
                continue;
            }
            if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tool_calls {
                    let tc_id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let func_name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    if !tc_id.is_empty() && !func_name.is_empty() {
                        tc_map.insert(tc_id.to_string(), func_name.to_string());
                    }
                }
            }
        }
        tc_map
    }

    /// Replace old tool result messages with compact references.
    pub fn mask_old_observations(&self, messages: &mut [ApiMessage], level: OptimizationLevel) {
        let recent_threshold = match level {
            OptimizationLevel::Mask => 6,
            OptimizationLevel::Aggressive => 3,
            _ => return,
        };

        // Find all tool result message indices
        let tool_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, msg)| msg.get("role").and_then(|v| v.as_str()) == Some("tool"))
            .map(|(i, _)| i)
            .collect();

        if tool_indices.len() <= recent_threshold {
            return;
        }

        let tc_map = Self::build_tool_call_map(messages);
        let old_count = tool_indices.len() - recent_threshold;
        let old_indices: HashSet<usize> = tool_indices[..old_count].iter().copied().collect();
        let mut masked_count = 0u32;

        for &i in &old_indices {
            let content = messages[i]
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if content.starts_with("[ref:") {
                continue;
            }
            let tool_call_id = messages[i]
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let tool_name = tc_map.get(&tool_call_id).map(|s| s.as_str()).unwrap_or("");
            if PROTECTED_TOOL_TYPES.contains(&tool_name) {
                continue;
            }
            messages[i].insert(
                "content".to_string(),
                serde_json::Value::String(format!(
                    "[ref: tool result {tool_call_id} — see history]"
                )),
            );
            masked_count += 1;
        }

        if masked_count > 0 {
            info!(
                "Masked {} old tool results (level={}, kept recent {})",
                masked_count,
                level.as_str(),
                recent_threshold,
            );
        }
    }

    /// Strip old tool outputs while protecting the most recent ones.
    pub fn prune_old_tool_outputs(&self, messages: &mut [ApiMessage]) {
        // Collect tool result indices in reverse order
        let mut tool_indices: Vec<usize> = Vec::new();
        for i in (0..messages.len()).rev() {
            if messages[i].get("role").and_then(|v| v.as_str()) == Some("tool") {
                tool_indices.push(i);
            }
        }

        if tool_indices.is_empty() {
            return;
        }

        let tc_map = Self::build_tool_call_map(messages);
        let mut protected_tokens: u64 = 0;
        let mut protected_indices: HashSet<usize> = HashSet::new();

        for &idx in &tool_indices {
            let content = messages[idx]
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if content.starts_with("[ref:") || content == "[pruned]" {
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

        let mut pruned_count = 0u32;
        for &idx in &tool_indices {
            if protected_indices.contains(&idx) {
                continue;
            }
            let content = messages[idx]
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if content.starts_with("[ref:") || content == "[pruned]" {
                continue;
            }
            messages[idx].insert(
                "content".to_string(),
                serde_json::Value::String("[pruned]".to_string()),
            );
            pruned_count += 1;
        }

        if pruned_count > 0 {
            info!(
                "Pruned {} old tool outputs (protected {}, ~{}K tokens kept)",
                pruned_count,
                protected_indices.len(),
                protected_tokens / 1000,
            );
        }
    }

    /// Apply sliding window compaction for sessions with 500+ messages.
    ///
    /// Keeps the first message (system prompt) and the most recent
    /// `SLIDING_WINDOW_RECENT` messages, replacing everything in between
    /// with a compressed summary. This runs *before* the staged approach.
    pub fn sliding_window_compact(&mut self, messages: Vec<ApiMessage>) -> Vec<ApiMessage> {
        if messages.len() < SLIDING_WINDOW_THRESHOLD {
            return messages;
        }

        let keep_start = 1; // preserve first message
        let keep_end = messages.len().saturating_sub(SLIDING_WINDOW_RECENT);

        if keep_end <= keep_start {
            return messages;
        }

        let head = &messages[..keep_start];
        let middle = &messages[keep_start..keep_end];
        let tail = &messages[keep_end..];

        let summary_text = Self::fallback_summary(middle);
        let artifact_summary = self.artifact_index.as_summary();
        let mut full_summary = format!(
            "[SLIDING WINDOW SUMMARY — {msg_count} messages compressed]\n{summary_text}",
            msg_count = middle.len(),
        );
        if !artifact_summary.is_empty() {
            full_summary = format!("{full_summary}\n\n{artifact_summary}");
        }

        let mut summary_msg = ApiMessage::new();
        summary_msg.insert(
            "role".to_string(),
            serde_json::Value::String("user".to_string()),
        );
        summary_msg.insert(
            "content".to_string(),
            serde_json::Value::String(full_summary),
        );

        let mut result = Vec::with_capacity(head.len() + 1 + tail.len());
        result.extend_from_slice(head);
        result.push(summary_msg);
        result.extend_from_slice(tail);

        info!(
            "Sliding window compaction: {} -> {} messages (compressed {} middle, kept {} recent)",
            messages.len(),
            result.len(),
            middle.len(),
            tail.len(),
        );

        result
    }

    /// Summarize verbose tool outputs (>500 chars) with 2-3 line summaries.
    ///
    /// Replaces long tool outputs with a compact summary preserving the tool
    /// name, success/failure status, and first/last lines. Protected tool
    /// types and already-processed outputs are skipped.
    pub fn summarize_verbose_tool_outputs(&self, messages: &mut [ApiMessage]) {
        let tc_map = Self::build_tool_call_map(messages);
        let mut summarized_count = 0u32;

        for msg in messages.iter_mut() {
            if msg.get("role").and_then(|v| v.as_str()) != Some("tool") {
                continue;
            }
            let content = msg
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if content.len() <= TOOL_OUTPUT_SUMMARIZE_THRESHOLD {
                continue;
            }
            if content.starts_with("[ref:")
                || content == "[pruned]"
                || content.starts_with("[summary:")
            {
                continue;
            }

            let tool_call_id = msg
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tool_name = tc_map
                .get(&tool_call_id)
                .map(|s| s.as_str())
                .unwrap_or("tool");

            if PROTECTED_TOOL_TYPES.contains(&tool_name) {
                continue;
            }

            let summary = summarize_tool_output(tool_name, &content);
            msg.insert(
                "content".to_string(),
                serde_json::Value::String(summary),
            );
            summarized_count += 1;
        }

        if summarized_count > 0 {
            info!(
                "Summarized {} verbose tool outputs (>{} chars)",
                summarized_count, TOOL_OUTPUT_SUMMARIZE_THRESHOLD,
            );
        }
    }

    /// Compact older messages into a summary, preserving recent context.
    ///
    /// Returns the compacted message list. Uses a fallback summary since
    /// LLM-powered summarization requires an HTTP client (handled at a higher layer).
    pub fn compact(&mut self, messages: Vec<ApiMessage>, _system_prompt: &str) -> Vec<ApiMessage> {
        if messages.len() <= 4 {
            return messages;
        }

        let keep_recent = (messages.len() / 3).clamp(2, 5);
        let split_point = messages.len() - keep_recent;

        let head = &messages[..1];
        let middle = &messages[1..split_point];
        let tail = &messages[split_point..];

        if middle.is_empty() {
            return messages;
        }

        let summary_text = Self::fallback_summary(middle);
        let artifact_summary = self.artifact_index.as_summary();
        let mut full_summary = format!("[CONVERSATION SUMMARY]\n{summary_text}");
        if !artifact_summary.is_empty() {
            full_summary = format!("{full_summary}\n\n{artifact_summary}");
        }

        let mut summary_msg = ApiMessage::new();
        summary_msg.insert(
            "role".to_string(),
            serde_json::Value::String("user".to_string()),
        );
        summary_msg.insert(
            "content".to_string(),
            serde_json::Value::String(full_summary),
        );

        let mut compacted = Vec::with_capacity(head.len() + 1 + tail.len());
        compacted.extend_from_slice(head);
        compacted.push(summary_msg);
        compacted.extend_from_slice(tail);

        info!(
            "Compacted {} messages -> {} (removed {}, kept {} recent)",
            messages.len(),
            compacted.len(),
            middle.len(),
            keep_recent,
        );

        // Invalidate calibration
        self.api_prompt_tokens = 0;
        self.msg_count_at_calibration = 0;
        self.warned_70 = false;
        self.warned_80 = false;
        self.warned_90 = false;

        compacted
    }

    /// Create a basic summary without an LLM call.
    pub fn fallback_summary(messages: &[ApiMessage]) -> String {
        let mut parts = Vec::new();
        let mut total = 0usize;
        for msg in messages {
            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
            if !content.is_empty() && (role == "user" || role == "assistant") {
                let snippet: String = content.chars().take(200).collect();
                parts.push(format!("- [{role}] {snippet}"));
                total += snippet.len();
                if total > 2000 {
                    let remaining = messages.len().saturating_sub(parts.len());
                    parts.push(format!("... ({remaining} more messages)"));
                    break;
                }
            }
        }
        parts.join("\n")
    }

    /// Estimate total tokens across messages and system prompt.
    ///
    /// Uses the improved `count_tokens` heuristic (cl100k_base approximation)
    /// instead of the naive `chars / 4`.
    fn count_message_tokens(messages: &[ApiMessage], system_prompt: &str) -> u64 {
        let mut total = count_tokens(system_prompt) as u64;
        for msg in messages {
            total += msg_token_count(msg) as u64;
        }
        total
    }

    fn update_token_count(&mut self, messages: &[ApiMessage], system_prompt: &str) {
        if self.api_prompt_tokens > 0 {
            let new_msg_count = messages.len().saturating_sub(self.msg_count_at_calibration);
            if new_msg_count > 0 {
                let start = messages.len() - new_msg_count;
                let delta = Self::count_message_tokens(&messages[start..], "");
                self.last_token_count = self.api_prompt_tokens + delta;
            } else {
                self.last_token_count = self.api_prompt_tokens;
            }
        } else {
            self.last_token_count = Self::count_message_tokens(messages, system_prompt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(role: &str, content: &str) -> ApiMessage {
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

    fn make_tool_msg(tool_call_id: &str, content: &str) -> ApiMessage {
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

    fn make_assistant_with_tc(tool_calls: Vec<(&str, &str)>) -> ApiMessage {
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

    #[test]
    fn test_optimization_levels() {
        let mut compactor = ContextCompactor::new(1000);

        // At 0% usage
        let messages = vec![make_msg("user", "hi")];
        assert_eq!(
            compactor.check_usage(&messages, ""),
            OptimizationLevel::None
        );

        // Force usage to 75% via API calibration
        compactor.update_from_api_usage(750, 1);
        assert_eq!(
            compactor.check_usage(&messages, ""),
            OptimizationLevel::Warning
        );

        // 85%
        compactor.update_from_api_usage(850, 1);
        assert_eq!(
            compactor.check_usage(&messages, ""),
            OptimizationLevel::Prune
        );

        // 95%
        compactor.update_from_api_usage(950, 1);
        assert_eq!(
            compactor.check_usage(&messages, ""),
            OptimizationLevel::Aggressive
        );

        // 99.5%
        compactor.update_from_api_usage(995, 1);
        assert_eq!(
            compactor.check_usage(&messages, ""),
            OptimizationLevel::Compact
        );
    }

    #[test]
    fn test_should_compact() {
        let mut compactor = ContextCompactor::new(1000);
        let messages = vec![make_msg("user", "hi")];
        assert!(!compactor.should_compact(&messages, ""));

        compactor.update_from_api_usage(995, 1);
        assert!(compactor.should_compact(&messages, ""));
    }

    #[test]
    fn test_mask_old_observations() {
        let compactor = ContextCompactor::new(100_000);

        // Create messages: assistant with tool calls, then 8 tool results
        let mut messages = vec![make_msg("system", "system prompt")];
        let tc_ids: Vec<String> = (0..8).map(|i| format!("tc-{i}")).collect();
        let tc_pairs: Vec<(&str, &str)> = tc_ids.iter().map(|id| (id.as_str(), "bash")).collect();
        messages.push(make_assistant_with_tc(tc_pairs));
        for id in &tc_ids {
            messages.push(make_tool_msg(id, &"x".repeat(100)));
        }

        // Mask level: keep recent 6, mask 2
        compactor.mask_old_observations(&mut messages, OptimizationLevel::Mask);

        let masked: Vec<_> = messages
            .iter()
            .filter(|m| {
                m.get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.starts_with("[ref:"))
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(masked.len(), 2);
    }

    #[test]
    fn test_protected_tools_not_masked() {
        let compactor = ContextCompactor::new(100_000);

        let mut messages = vec![make_msg("system", "sys")];
        let tc_ids: Vec<String> = (0..10).map(|i| format!("tc-{i}")).collect();
        let mut names = vec!["read_file"];
        for _ in 1..10 {
            names.push("bash");
        }
        let pairs: Vec<(&str, &str)> = tc_ids
            .iter()
            .zip(names.iter())
            .map(|(id, name)| (id.as_str(), *name))
            .collect();
        messages.push(make_assistant_with_tc(pairs));
        for id in &tc_ids {
            messages.push(make_tool_msg(id, &"x".repeat(100)));
        }

        compactor.mask_old_observations(&mut messages, OptimizationLevel::Aggressive);

        // tc-0 is read_file and should NOT be masked
        let tc0_msg = messages
            .iter()
            .find(|m| m.get("tool_call_id").and_then(|v| v.as_str()) == Some("tc-0"))
            .unwrap();
        let content = tc0_msg.get("content").and_then(|v| v.as_str()).unwrap();
        assert!(!content.starts_with("[ref:"));
    }

    #[test]
    fn test_compact_small_conversation() {
        let mut compactor = ContextCompactor::new(100_000);
        let messages = vec![
            make_msg("system", "sys"),
            make_msg("user", "hello"),
            make_msg("assistant", "hi"),
        ];
        // Should not compact if <= 4 messages
        let result = compactor.compact(messages.clone(), "sys");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_compact_large_conversation() {
        let mut compactor = ContextCompactor::new(100_000);
        let mut messages = vec![make_msg("system", "sys")];
        for i in 0..20 {
            messages.push(make_msg("user", &format!("question {i}")));
            messages.push(make_msg("assistant", &format!("answer {i}")));
        }
        let original_len = messages.len();
        let result = compactor.compact(messages, "sys");
        assert!(result.len() < original_len);
        // First message preserved
        assert_eq!(
            result[0].get("role").and_then(|v| v.as_str()),
            Some("system")
        );
        // Summary message present
        let has_summary = result.iter().any(|m| {
            m.get("content")
                .and_then(|v| v.as_str())
                .map(|s| s.contains("[CONVERSATION SUMMARY]"))
                .unwrap_or(false)
        });
        assert!(has_summary);
    }

    #[test]
    fn test_artifact_index() {
        let mut idx = ArtifactIndex::new();
        assert!(idx.is_empty());

        idx.record("src/main.rs", "created", "50 lines");
        assert_eq!(idx.len(), 1);

        idx.record("src/main.rs", "modified", "added tests");
        assert_eq!(idx.len(), 1); // Same file, updated in-place
        let entry = idx.entries.get("src/main.rs").unwrap();
        assert_eq!(entry.operation_count, 2);
        assert_eq!(entry.operations_seen, vec!["created", "modified"]);

        let summary = idx.as_summary();
        assert!(summary.contains("src/main.rs"));
        assert!(summary.contains("created, modified"));
    }

    #[test]
    fn test_artifact_index_json_roundtrip() {
        let mut idx = ArtifactIndex::new();
        idx.record("src/main.rs", "created", "50 lines");
        idx.record("src/lib.rs", "modified", "added tests");

        let json = idx.to_json();
        let restored = ArtifactIndex::from_json(&json).unwrap();
        assert_eq!(restored.len(), 2);
        let entry = restored.entries.get("src/main.rs").unwrap();
        assert_eq!(entry.operation_count, 1);
        assert_eq!(entry.last_operation, "created");
    }

    #[test]
    fn test_compactor_save_load_artifact_index() {
        let mut compactor = ContextCompactor::new(100_000);
        compactor
            .artifact_index
            .record("src/app.rs", "created", "new file");
        compactor
            .artifact_index
            .record("src/app.rs", "modified", "added fn");

        // Save to metadata
        let mut metadata = std::collections::HashMap::new();
        compactor.save_artifact_index(&mut metadata);
        assert!(metadata.contains_key("artifact_index"));

        // Load into a fresh compactor
        let mut compactor2 = ContextCompactor::new(100_000);
        assert!(compactor2.artifact_index.is_empty());
        compactor2.load_artifact_index(&metadata);
        assert_eq!(compactor2.artifact_index.len(), 1);
        let entry = compactor2.artifact_index.entries.get("src/app.rs").unwrap();
        assert_eq!(entry.operation_count, 2);
    }

    #[test]
    fn test_artifact_index_from_invalid_json() {
        let invalid = serde_json::json!("not an object");
        assert!(ArtifactIndex::from_json(&invalid).is_none());
    }

    #[test]
    fn test_prune_old_tool_outputs() {
        let compactor = ContextCompactor::new(100_000);

        let mut messages = vec![make_msg("system", "sys")];
        // Many tool calls with large outputs
        let tc_ids: Vec<String> = (0..20).map(|i| format!("tc-{i}")).collect();
        let pairs: Vec<(&str, &str)> = tc_ids.iter().map(|id| (id.as_str(), "bash")).collect();
        messages.push(make_assistant_with_tc(pairs));
        for id in &tc_ids {
            // Each tool output is large enough to exceed budget
            messages.push(make_tool_msg(id, &"x".repeat(20_000)));
        }

        compactor.prune_old_tool_outputs(&mut messages);

        let pruned_count = messages
            .iter()
            .filter(|m| m.get("content").and_then(|v| v.as_str()) == Some("[pruned]"))
            .count();
        assert!(pruned_count > 0, "Some messages should have been pruned");
    }

    #[test]
    fn test_fallback_summary() {
        let messages = vec![
            make_msg("user", "What is Rust?"),
            make_msg("assistant", "Rust is a systems programming language."),
            make_msg("user", "Tell me more."),
        ];
        let summary = ContextCompactor::fallback_summary(&messages);
        assert!(summary.contains("[user] What is Rust?"));
        assert!(summary.contains("[assistant] Rust is a systems programming language."));
    }
}
