//! The main context compactor state machine.

mod stages;
mod summary;

use tracing::{debug, info, warn};

use super::artifacts::ArtifactIndex;
use super::levels::OptimizationLevel;
use super::preview::msg_token_count;
use super::tokens::count_tokens;
use super::{ApiMessage, STAGE_AGGRESSIVE, STAGE_COMPACT, STAGE_MASK, STAGE_PRUNE, STAGE_WARNING};

/// Auto-compacts conversation history when approaching context limits.
pub struct ContextCompactor {
    max_context: u64,
    last_token_count: u64,
    pub(super) api_prompt_tokens: u64,
    pub(super) msg_count_at_calibration: usize,
    pub(super) warned_70: bool,
    pub(super) warned_80: bool,
    pub(super) warned_90: bool,
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

    /// Estimate total tokens across messages and system prompt.
    ///
    /// Uses the improved `count_tokens` heuristic (cl100k_base approximation)
    /// instead of the naive `chars / 4`.
    pub(super) fn count_message_tokens(messages: &[ApiMessage], system_prompt: &str) -> u64 {
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
mod tests;
