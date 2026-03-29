//! Strips provider-specific tokens from model responses.
//!
//! Mirrors `opendev/core/agents/components/response/cleaner.py`.

use regex::Regex;
use std::sync::LazyLock;

/// Compiled cleanup patterns, initialized once.
static CLEANUP_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        // Match chat template tokens like <|im_end|>, <|im_user|>, etc.
        // All patterns below are known-good compile-time regex literals.
        (
            Regex::new(r"<\|[^|]+\|>").expect("valid regex: chat template tokens"),
            "",
        ),
        (
            Regex::new(r"</?tool_call>").expect("valid regex: tool_call tags"),
            "",
        ),
        (
            Regex::new(r"</?tool_response>").expect("valid regex: tool_response tags"),
            "",
        ),
        (
            Regex::new(r"<function=[^>]+>").expect("valid regex: function tags"),
            "",
        ),
        (
            Regex::new(r"</?parameter[^>]*>").expect("valid regex: parameter tags"),
            "",
        ),
        // Strip echoed system/internal markers (defense-in-depth)
        (
            Regex::new(r"(?m)^\[SYSTEM\].*$\n?").expect("valid regex: system markers"),
            "",
        ),
        (
            Regex::new(r"(?m)^\[INTERNAL\].*$\n?").expect("valid regex: internal markers"),
            "",
        ),
    ]
});

/// Strips provider-specific tokens from model responses.
#[derive(Debug, Clone, Default)]
pub struct ResponseCleaner;

impl ResponseCleaner {
    /// Create a new response cleaner.
    pub fn new() -> Self {
        Self
    }

    /// Return the sanitized content string.
    ///
    /// Returns `None` if the input is `None` or empty.
    pub fn clean(&self, content: Option<&str>) -> Option<String> {
        let content = content?;
        if content.is_empty() {
            return None;
        }

        let mut cleaned = content.to_string();
        for (pattern, replacement) in CLEANUP_PATTERNS.iter() {
            cleaned = pattern.replace_all(&cleaned, *replacement).to_string();
        }

        let trimmed = cleaned.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }
}

#[cfg(test)]
#[path = "cleaner_tests.rs"]
mod tests;
