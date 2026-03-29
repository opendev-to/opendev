//! Structured error types for OpenDev.
//!
//! Provides typed error classes with structured fields for better retry logic,
//! error-specific recovery, and comprehensive provider error classification.
//! Ported from `opendev/core/errors.py`.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// High-level error category for classification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    ContextOverflow,
    OutputLength,
    RateLimit,
    Auth,
    Api,
    Gateway,
    Permission,
    EditMismatch,
    FileNotFound,
    Timeout,
    Unknown,
}

/// Strategy for recovering from an error.
///
/// Each error category maps to a recommended recovery strategy that callers
/// can use to decide how to handle failures automatically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecoveryStrategy {
    /// Retry the operation after a delay.
    Retry {
        /// Milliseconds to wait before retrying.
        delay_ms: u64,
        /// Maximum number of retry attempts.
        max_attempts: u32,
    },
    /// Fall back to an alternative model.
    FallbackModel {
        /// The model identifier to fall back to.
        model: String,
    },
    /// Reduce the context window and retry.
    ReduceContext,
    /// Require user intervention with a descriptive message.
    UserIntervention {
        /// Description of what the user should do.
        message: String,
    },
}

impl RecoveryStrategy {
    /// Serialize the recovery strategy to a JSON value.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({"type": "unknown"}))
    }
}

/// Base structured error with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredError {
    pub category: ErrorCategory,
    pub message: String,
    pub is_retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_error: Option<String>,
    /// For context overflow: how many tokens were in the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<u64>,
    /// For context overflow: what the model limit is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_limit: Option<u64>,
    /// For rate limit: seconds to wait before retrying.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<f64>,
}

impl StructuredError {
    /// Whether this error should trigger context compaction.
    pub fn should_compact(&self) -> bool {
        self.category == ErrorCategory::ContextOverflow
    }

    /// Whether the operation should be retried.
    pub fn should_retry(&self) -> bool {
        self.is_retryable
    }

    /// Return a stable error code for this error.
    ///
    /// Error codes follow the pattern `EXXXX_DESCRIPTION`:
    /// - E1xxx: Rate limiting and quota errors
    /// - E2xxx: Tool and timeout errors
    /// - E3xxx: Context and token errors
    /// - E4xxx: Authentication and permission errors
    /// - E5xxx: Gateway and network errors
    /// - E9xxx: Unknown/unclassified errors
    pub fn error_code(&self) -> &str {
        match self.category {
            ErrorCategory::RateLimit => "E1001_RATE_LIMIT",
            ErrorCategory::Timeout => "E2001_TOOL_TIMEOUT",
            ErrorCategory::ContextOverflow => "E3001_CONTEXT_OVERFLOW",
            ErrorCategory::OutputLength => "E3002_OUTPUT_LENGTH",
            ErrorCategory::Auth => "E4001_AUTH_FAILED",
            ErrorCategory::Permission => "E4002_PERMISSION_DENIED",
            ErrorCategory::Gateway => "E5001_GATEWAY_ERROR",
            ErrorCategory::Api => "E5002_API_ERROR",
            ErrorCategory::EditMismatch => "E6001_EDIT_MISMATCH",
            ErrorCategory::FileNotFound => "E6002_FILE_NOT_FOUND",
            ErrorCategory::Unknown => "E9001_UNKNOWN",
        }
    }

    /// Serialize this error to a structured JSON value for reporting.
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "error_code": self.error_code(),
            "category": self.category,
            "message": self.message,
            "is_retryable": self.is_retryable,
        });
        let map = obj.as_object_mut().expect("json object");
        if let Some(sc) = self.status_code {
            map.insert("status_code".into(), serde_json::json!(sc));
        }
        if let Some(ref p) = self.provider {
            map.insert("provider".into(), serde_json::json!(p));
        }
        if let Some(ref oe) = self.original_error {
            map.insert("original_error".into(), serde_json::json!(oe));
        }
        if let Some(tc) = self.token_count {
            map.insert("token_count".into(), serde_json::json!(tc));
        }
        if let Some(tl) = self.token_limit {
            map.insert("token_limit".into(), serde_json::json!(tl));
        }
        if let Some(ra) = self.retry_after {
            map.insert("retry_after".into(), serde_json::json!(ra));
        }
        let strategy = self.recovery_strategy();
        map.insert("recovery_strategy".into(), strategy.to_json());
        obj
    }

    /// Return the recommended recovery strategy for this error.
    pub fn recovery_strategy(&self) -> RecoveryStrategy {
        match self.category {
            ErrorCategory::RateLimit => {
                let delay = self
                    .retry_after
                    .map(|s| (s * 1000.0) as u64)
                    .unwrap_or(5000);
                RecoveryStrategy::Retry {
                    delay_ms: delay,
                    max_attempts: 3,
                }
            }
            ErrorCategory::Timeout => RecoveryStrategy::Retry {
                delay_ms: 2000,
                max_attempts: 2,
            },
            ErrorCategory::ContextOverflow => RecoveryStrategy::ReduceContext,
            ErrorCategory::OutputLength => RecoveryStrategy::Retry {
                delay_ms: 0,
                max_attempts: 1,
            },
            ErrorCategory::Auth => RecoveryStrategy::UserIntervention {
                message: "Check your API key and authentication settings.".into(),
            },
            ErrorCategory::Permission => RecoveryStrategy::UserIntervention {
                message: "Insufficient permissions. Check your access rights.".into(),
            },
            ErrorCategory::Gateway => RecoveryStrategy::Retry {
                delay_ms: 3000,
                max_attempts: 3,
            },
            ErrorCategory::Api => {
                if self.is_retryable {
                    RecoveryStrategy::Retry {
                        delay_ms: 2000,
                        max_attempts: 3,
                    }
                } else {
                    RecoveryStrategy::FallbackModel {
                        model: "default".into(),
                    }
                }
            }
            ErrorCategory::EditMismatch => RecoveryStrategy::UserIntervention {
                message: "The edit target was not found. Review the file content.".into(),
            },
            ErrorCategory::FileNotFound => RecoveryStrategy::UserIntervention {
                message: "File not found. Check the path and try again.".into(),
            },
            ErrorCategory::Unknown => RecoveryStrategy::UserIntervention {
                message: "An unexpected error occurred. Please try again.".into(),
            },
        }
    }

    /// Create a generic API error.
    pub fn api(message: impl Into<String>, status_code: Option<u16>) -> Self {
        let code = status_code;
        Self {
            category: if code.is_some() {
                ErrorCategory::Api
            } else {
                ErrorCategory::Unknown
            },
            message: message.into(),
            is_retryable: matches!(code, Some(500 | 502 | 503 | 504)),
            status_code: code,
            provider: None,
            original_error: None,
            token_count: None,
            token_limit: None,
            retry_after: None,
        }
    }

    /// Create a context overflow error.
    pub fn context_overflow(
        message: impl Into<String>,
        provider: Option<String>,
        token_count: Option<u64>,
        token_limit: Option<u64>,
    ) -> Self {
        let msg = message.into();
        Self {
            category: ErrorCategory::ContextOverflow,
            message: msg.clone(),
            is_retryable: true,
            status_code: None,
            provider,
            original_error: Some(msg),
            token_count,
            token_limit,
            retry_after: None,
        }
    }

    /// Create an output length error.
    pub fn output_length(message: impl Into<String>) -> Self {
        Self {
            category: ErrorCategory::OutputLength,
            message: message.into(),
            is_retryable: true,
            status_code: None,
            provider: None,
            original_error: None,
            token_count: None,
            token_limit: None,
            retry_after: None,
        }
    }

    /// Create a rate limit error.
    pub fn rate_limit(
        message: impl Into<String>,
        provider: Option<String>,
        retry_after: Option<f64>,
    ) -> Self {
        let msg = message.into();
        Self {
            category: ErrorCategory::RateLimit,
            message: msg.clone(),
            is_retryable: true,
            status_code: None,
            provider,
            original_error: Some(msg),
            token_count: None,
            token_limit: None,
            retry_after,
        }
    }

    /// Create an authentication error.
    pub fn auth(
        message: impl Into<String>,
        status_code: Option<u16>,
        provider: Option<String>,
    ) -> Self {
        let msg = message.into();
        Self {
            category: ErrorCategory::Auth,
            message: msg.clone(),
            is_retryable: false,
            status_code,
            provider,
            original_error: Some(msg),
            token_count: None,
            token_limit: None,
            retry_after: None,
        }
    }

    /// Create a gateway error.
    pub fn gateway(
        message: impl Into<String>,
        status_code: Option<u16>,
        provider: Option<String>,
        original_error: Option<String>,
    ) -> Self {
        Self {
            category: ErrorCategory::Gateway,
            message: message.into(),
            is_retryable: true,
            status_code,
            provider,
            original_error,
            token_count: None,
            token_limit: None,
            retry_after: None,
        }
    }
}

impl std::fmt::Display for StructuredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.error_code(), self.message)
    }
}

impl std::error::Error for StructuredError {}

// ---------------------------------------------------------------------------
// Provider error pattern library
// ---------------------------------------------------------------------------

/// Compiled regex patterns for each error category.
struct PatternSet {
    overflow: Vec<Regex>,
    rate_limit: Vec<Regex>,
    auth: Vec<Regex>,
    gateway: Vec<Regex>,
}

fn compile_patterns(patterns: &[&str]) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(|p| Regex::new(&format!("(?i){}", p)).ok())
        .collect()
}

static PATTERNS: LazyLock<PatternSet> = LazyLock::new(|| {
    PatternSet {
        overflow: compile_patterns(&[
            // Anthropic
            r"prompt is too long",
            r"max_tokens_exceeded",
            r"context length.*exceeded",
            r"maximum context length",
            // OpenAI
            r"maximum context length.*is \d+ tokens",
            r"This model's maximum context length is",
            r"reduce the length of the messages",
            r"context_length_exceeded",
            // Google
            r"exceeds the maximum.*tokens",
            r"RESOURCE_EXHAUSTED.*token",
            r"GenerateContentRequest.*too large",
            // Azure
            r"Tokens in prompt.*exceed.*limit",
            // Generic
            r"token limit",
            r"too many tokens",
            r"context.*too long",
            r"input.*too long",
            r"prompt.*too large",
        ]),
        rate_limit: compile_patterns(&[
            r"rate.?limit",
            r"too many requests",
            r"429",
            r"quota exceeded",
            r"capacity",
            r"overloaded",
        ]),
        auth: compile_patterns(&[
            r"invalid.*api.?key",
            r"authentication",
            r"unauthorized",
            r"invalid.*token",
            r"api key.*invalid",
        ]),
        gateway: compile_patterns(&[
            r"<!doctype html",
            r"<html",
            r"502 Bad Gateway",
            r"503 Service Unavailable",
            r"504 Gateway Timeout",
            r"CloudFlare",
            r"nginx",
        ]),
    }
});

/// Classify an API error into a structured error type.
///
/// Checks the raw error message against known patterns for context overflow,
/// rate limiting, authentication failures, and gateway/proxy issues across
/// all supported providers (Anthropic, OpenAI, Google, Azure).
pub fn classify_api_error(
    error_message: &str,
    status_code: Option<u16>,
    provider: Option<&str>,
) -> StructuredError {
    let patterns = &*PATTERNS;
    let provider_owned = provider.map(|s| s.to_string());

    // Check gateway patterns first (HTML responses)
    for re in &patterns.gateway {
        if re.is_match(error_message) {
            let friendly_msg = match status_code {
                Some(401) => {
                    "Authentication failed at gateway. Check your API key and proxy settings."
                        .to_string()
                }
                Some(403) => "Access denied at gateway. Check your permissions and proxy settings."
                    .to_string(),
                _ => "API returned an HTML error page. Check your proxy/VPN settings or try again."
                    .to_string(),
            };
            let truncated = if error_message.len() > 500 {
                &error_message[..500]
            } else {
                error_message
            };
            return StructuredError::gateway(
                friendly_msg,
                status_code,
                provider_owned,
                Some(truncated.to_string()),
            );
        }
    }

    // Context overflow
    for re in &patterns.overflow {
        if re.is_match(error_message) {
            return StructuredError::context_overflow(error_message, provider_owned, None, None);
        }
    }

    // Rate limiting
    for re in &patterns.rate_limit {
        if re.is_match(error_message) {
            static RETRY_AFTER_RE: LazyLock<Regex> = LazyLock::new(|| {
                Regex::new(r"(?i)retry.?after[:\s]+(\d+\.?\d*)")
                    .expect("valid regex: retry-after pattern")
            });
            let retry_after = RETRY_AFTER_RE
                .captures(error_message)
                .and_then(|caps| caps.get(1))
                .and_then(|m| m.as_str().parse::<f64>().ok());
            return StructuredError::rate_limit(error_message, provider_owned, retry_after);
        }
    }

    // Auth errors — check status code first, then patterns
    if matches!(status_code, Some(401 | 403)) {
        return StructuredError::auth(error_message, status_code, provider_owned);
    }
    for re in &patterns.auth {
        if re.is_match(error_message) {
            return StructuredError::auth(error_message, status_code, provider_owned);
        }
    }

    // Generic API error
    StructuredError {
        category: if status_code.is_some() {
            ErrorCategory::Api
        } else {
            ErrorCategory::Unknown
        },
        message: error_message.to_string(),
        is_retryable: matches!(status_code, Some(500 | 502 | 503 | 504)),
        status_code,
        provider: provider_owned,
        original_error: Some(error_message.to_string()),
        token_count: None,
        token_limit: None,
        retry_after: None,
    }
}

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
