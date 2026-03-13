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
mod tests {
    use super::*;

    #[test]
    fn test_clean_none() {
        let cleaner = ResponseCleaner::new();
        assert!(cleaner.clean(None).is_none());
    }

    #[test]
    fn test_clean_empty() {
        let cleaner = ResponseCleaner::new();
        assert!(cleaner.clean(Some("")).is_none());
    }

    #[test]
    fn test_clean_normal_text() {
        let cleaner = ResponseCleaner::new();
        assert_eq!(
            cleaner.clean(Some("Hello, world!")),
            Some("Hello, world!".to_string())
        );
    }

    #[test]
    fn test_clean_chat_template_tokens() {
        let cleaner = ResponseCleaner::new();
        let input = "Hello<|im_end|> world<|im_user|>";
        assert_eq!(cleaner.clean(Some(input)), Some("Hello world".to_string()));
    }

    #[test]
    fn test_clean_tool_call_tags() {
        let cleaner = ResponseCleaner::new();
        let input = "<tool_call>some content</tool_call>";
        assert_eq!(cleaner.clean(Some(input)), Some("some content".to_string()));
    }

    #[test]
    fn test_clean_tool_response_tags() {
        let cleaner = ResponseCleaner::new();
        let input = "<tool_response>result</tool_response>";
        assert_eq!(cleaner.clean(Some(input)), Some("result".to_string()));
    }

    #[test]
    fn test_clean_function_tags() {
        let cleaner = ResponseCleaner::new();
        let input = "<function=read_file>args</function>";
        // <function=read_file> is removed, </function> is not matched by the patterns
        // but "args</function>" remains
        let result = cleaner.clean(Some(input));
        assert!(result.is_some());
        assert!(!result.as_ref().unwrap().contains("<function=read_file>"));
    }

    #[test]
    fn test_clean_parameter_tags() {
        let cleaner = ResponseCleaner::new();
        let input = "<parameter name=\"path\">value</parameter>";
        assert_eq!(cleaner.clean(Some(input)), Some("value".to_string()));
    }

    #[test]
    fn test_clean_mixed_tokens() {
        let cleaner = ResponseCleaner::new();
        let input = "Start <|im_start|>content<tool_call> end<|im_end|>";
        let result = cleaner.clean(Some(input)).unwrap();
        assert!(!result.contains("<|"));
        assert!(!result.contains("<tool_call>"));
        assert!(result.contains("Start"));
        assert!(result.contains("content"));
        assert!(result.contains("end"));
    }

    #[test]
    fn test_clean_only_tokens_returns_none() {
        let cleaner = ResponseCleaner::new();
        let input = "<|im_end|><|im_start|>";
        assert!(cleaner.clean(Some(input)).is_none());
    }

    #[test]
    fn test_clean_whitespace_trimming() {
        let cleaner = ResponseCleaner::new();
        let input = "  hello  <|im_end|>  ";
        assert_eq!(cleaner.clean(Some(input)), Some("hello".to_string()));
    }
}
