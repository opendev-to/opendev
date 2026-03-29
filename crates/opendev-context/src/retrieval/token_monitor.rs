//! Token counting utilities for context summaries.
//!
//! Uses a simple heuristic (text length / 4) as an approximation
//! of token count, avoiding the need for a full tokenizer dependency.

/// Stateless token counter using a character-based heuristic.
#[derive(Debug, Clone, Default)]
pub struct ContextTokenMonitor;

impl ContextTokenMonitor {
    /// Create a new token monitor.
    pub fn new() -> Self {
        Self
    }

    /// Estimate the number of tokens in the given text.
    ///
    /// Uses a simple heuristic: `text.len() / 4`, which approximates
    /// the average token length for English text with code.
    pub fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
}

#[cfg(test)]
#[path = "token_monitor_tests.rs"]
mod tests;
