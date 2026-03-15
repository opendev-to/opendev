//! Shared types for HTTP operations.

use serde::{Deserialize, Serialize};

/// Errors that can occur during HTTP operations.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Interrupted by user")]
    Interrupted,

    #[error("All retries exhausted: {message}")]
    RetriesExhausted { message: String },

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

/// Result of an HTTP request attempt.
#[derive(Debug)]
pub struct HttpResult {
    /// Whether the request completed successfully.
    pub success: bool,
    /// HTTP status code, if a response was received.
    pub status: Option<u16>,
    /// Response body, if available.
    pub body: Option<serde_json::Value>,
    /// Error message, if the request failed.
    pub error: Option<String>,
    /// Whether the request was interrupted by the user.
    pub interrupted: bool,
    /// Whether the failure is transient and worth retrying.
    pub retryable: bool,
    /// Unique request identifier for end-to-end tracing.
    /// Propagated via the `X-Request-ID` header.
    pub request_id: Option<String>,
    /// Value of the `Retry-After` response header, if present.
    /// Used to honor server-requested retry delays on 429/503 responses.
    pub retry_after: Option<String>,
    /// Value of the `retry-after-ms` response header, if present.
    /// More precise than `Retry-After` (milliseconds instead of seconds).
    pub retry_after_ms: Option<String>,
}

impl HttpResult {
    /// Create a successful result.
    pub fn ok(status: u16, body: serde_json::Value) -> Self {
        Self {
            success: true,
            status: Some(status),
            body: Some(body),
            error: None,
            interrupted: false,
            retryable: false,
            request_id: None,
            retry_after: None,
            retry_after_ms: None,
        }
    }

    /// Create a failed result.
    pub fn fail(error: impl Into<String>, retryable: bool) -> Self {
        Self {
            success: false,
            status: None,
            body: None,
            error: Some(error.into()),
            interrupted: false,
            retryable,
            request_id: None,
            retry_after: None,
            retry_after_ms: None,
        }
    }

    /// Create an interrupted result.
    pub fn interrupted() -> Self {
        Self {
            success: false,
            status: None,
            body: None,
            error: Some("Interrupted by user".into()),
            interrupted: true,
            retryable: false,
            request_id: None,
            retry_after: None,
            retry_after_ms: None,
        }
    }

    /// Create a result from an HTTP response with a retryable status.
    pub fn retryable_status(
        status: u16,
        body: Option<serde_json::Value>,
        retry_after: Option<String>,
    ) -> Self {
        Self {
            success: false,
            status: Some(status),
            body,
            error: Some(format!("HTTP {status}")),
            interrupted: false,
            retryable: true,
            request_id: None,
            retry_after,
            retry_after_ms: None,
        }
    }

    /// Attach a request ID to this result for tracing.
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
}

/// Configuration for retry behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not counting the initial request).
    pub max_retries: u32,
    /// Base delays in seconds for exponential backoff.
    pub retry_delays: Vec<f64>,
    /// HTTP status codes that trigger a retry.
    pub retryable_status_codes: Vec<u16>,
    /// Initial delay in milliseconds for exponential backoff.
    #[serde(default = "default_initial_delay_ms")]
    pub initial_delay_ms: u64,
    /// Multiplier for exponential backoff (delay *= factor each attempt).
    #[serde(default = "default_backoff_factor")]
    pub backoff_factor: f64,
    /// Maximum delay in milliseconds (cap for exponential growth).
    #[serde(default = "default_max_delay_ms")]
    pub max_delay_ms: u64,
}

fn default_initial_delay_ms() -> u64 {
    2000
}
fn default_backoff_factor() -> f64 {
    2.0
}
fn default_max_delay_ms() -> u64 {
    30000
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_delays: vec![1.0, 2.0, 4.0],
            retryable_status_codes: vec![429, 500, 502, 503, 504],
            initial_delay_ms: 2000,
            backoff_factor: 2.0,
            max_delay_ms: 30000,
        }
    }
}

impl RetryConfig {
    /// Get the delay for a given attempt index (0-based).
    ///
    /// Uses exponential backoff: `initial_delay_ms * backoff_factor^attempt`,
    /// capped at `max_delay_ms`. Falls back to the legacy `retry_delays` array
    /// if `initial_delay_ms` is 0.
    pub fn delay_for_attempt(&self, attempt: u32) -> std::time::Duration {
        if self.initial_delay_ms > 0 {
            let delay_ms =
                self.initial_delay_ms as f64 * self.backoff_factor.powi(attempt as i32);
            let capped_ms = delay_ms.min(self.max_delay_ms as f64) as u64;
            return std::time::Duration::from_millis(capped_ms);
        }
        // Legacy fallback: use fixed delay array
        let idx = (attempt as usize).min(self.retry_delays.len().saturating_sub(1));
        let secs = self.retry_delays.get(idx).copied().unwrap_or(4.0);
        std::time::Duration::from_secs_f64(secs)
    }

    /// Check if a status code is retryable.
    pub fn is_retryable_status(&self, status: u16) -> bool {
        self.retryable_status_codes.contains(&status)
    }
}

/// Classify an API error response and return a human-readable retry reason.
///
/// Returns `Some(message)` if the error is retryable, `None` if it should not be retried.
pub fn classify_retryable_error(
    status: Option<u16>,
    body: Option<&serde_json::Value>,
) -> Option<String> {
    // Check status code first
    match status {
        Some(429) => {
            if let Some(body) = body
                && let Some(msg) = extract_error_message(body)
            {
                if msg.contains("rate_limit") || msg.contains("Rate") {
                    return Some("Rate Limited".to_string());
                }
                if msg.contains("too_many_requests") || msg.contains("Too Many") {
                    return Some("Too Many Requests".to_string());
                }
            }
            return Some("Rate Limited".to_string());
        }
        Some(529) => return Some("Provider is overloaded".to_string()),
        Some(503) => {
            if let Some(body) = body
                && let Some(msg) = extract_error_message(body)
            {
                if msg.contains("overloaded") || msg.contains("Overloaded") {
                    return Some("Provider is overloaded".to_string());
                }
                if msg.contains("unavailable") || msg.contains("exhausted") {
                    return Some("Provider is overloaded".to_string());
                }
            }
            return Some("Service Unavailable".to_string());
        }
        Some(500) => return Some("Internal Server Error".to_string()),
        Some(502) => return Some("Bad Gateway".to_string()),
        Some(504) => return Some("Gateway Timeout".to_string()),
        _ => {}
    }

    // Check body for retryable error patterns even without a matching status
    if let Some(body) = body
        && let Some(msg) = extract_error_message(body)
        && (msg.contains("overloaded") || msg.contains("Overloaded"))
    {
        return Some("Provider is overloaded".to_string());
    }

    None
}

/// Parse a `Retry-After` or `retry-after-ms` header value into a Duration.
///
/// Supports:
/// - `retry-after-ms` header (milliseconds, if provided separately)
/// - `Retry-After` as seconds (integer or float)
/// - `Retry-After` as HTTP date (RFC 2822)
///
/// Returns `None` if parsing fails.
pub fn parse_retry_after(
    retry_after: Option<&str>,
    retry_after_ms: Option<&str>,
) -> Option<std::time::Duration> {
    // Prefer retry-after-ms (more precise)
    if let Some(ms_str) = retry_after_ms
        && let Ok(ms) = ms_str.parse::<u64>()
    {
        return Some(std::time::Duration::from_millis(ms));
    }

    let val = retry_after?;

    // Try parsing as seconds (integer or float)
    if let Ok(secs) = val.parse::<f64>()
        && secs > 0.0
    {
        return Some(std::time::Duration::from_secs_f64(secs));
    }

    // Try parsing as HTTP date (RFC 2822 / RFC 7231)
    // Example: "Wed, 21 Oct 2015 07:28:00 GMT"
    if val.contains(',')
        && val.contains("GMT")
        && let Ok(date) = httpdate::parse_http_date(val)
        && let Ok(duration) = date.duration_since(std::time::SystemTime::now())
    {
        return Some(duration);
    }

    None
}

/// Extract an error message from an API error response body.
fn extract_error_message(body: &serde_json::Value) -> Option<String> {
    // OpenAI: {"error": {"message": "...", "type": "...", "code": "..."}}
    if let Some(err) = body.get("error") {
        if let Some(msg) = err.get("message").and_then(|v| v.as_str()) {
            return Some(msg.to_string());
        }
        if let Some(code) = err.get("code").and_then(|v| v.as_str()) {
            return Some(code.to_string());
        }
        if let Some(err_type) = err.get("type").and_then(|v| v.as_str()) {
            return Some(err_type.to_string());
        }
        if let Some(msg) = err.as_str() {
            return Some(msg.to_string());
        }
    }
    // Anthropic: {"type": "error", "error": {"type": "...", "message": "..."}}
    if body.get("type").and_then(|v| v.as_str()) == Some("error")
        && let Some(err) = body.get("error")
        && let Some(msg) = err.get("message").and_then(|v| v.as_str())
    {
        return Some(msg.to_string());
    }
    // Generic message field
    body.get("message")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_result_ok() {
        let result = HttpResult::ok(200, serde_json::json!({"message": "hello"}));
        assert!(result.success);
        assert_eq!(result.status, Some(200));
        assert!(!result.interrupted);
        assert!(!result.retryable);
    }

    #[test]
    fn test_http_result_fail() {
        let result = HttpResult::fail("connection refused", true);
        assert!(!result.success);
        assert!(result.retryable);
        assert_eq!(result.error.as_deref(), Some("connection refused"));
    }

    #[test]
    fn test_http_result_interrupted() {
        let result = HttpResult::interrupted();
        assert!(!result.success);
        assert!(result.interrupted);
        assert!(!result.retryable);
    }

    #[test]
    fn test_http_result_retryable_status() {
        let result = HttpResult::retryable_status(429, None, None);
        assert!(!result.success);
        assert!(result.retryable);
        assert_eq!(result.status, Some(429));
    }

    #[test]
    fn test_http_result_retryable_status_with_retry_after() {
        let result = HttpResult::retryable_status(429, None, Some("30".to_string()));
        assert!(!result.success);
        assert!(result.retryable);
        assert_eq!(result.retry_after.as_deref(), Some("30"));
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert!(config.is_retryable_status(429));
        assert!(config.is_retryable_status(500));
        assert!(config.is_retryable_status(502));
        assert!(config.is_retryable_status(503));
        assert!(config.is_retryable_status(504));
        assert!(!config.is_retryable_status(404));
        assert_eq!(config.initial_delay_ms, 2000);
        assert_eq!(config.backoff_factor, 2.0);
        assert_eq!(config.max_delay_ms, 30000);
    }

    #[test]
    fn test_retry_config_exponential_backoff() {
        let config = RetryConfig::default();
        // 2000 * 2^0 = 2000ms
        assert_eq!(
            config.delay_for_attempt(0),
            std::time::Duration::from_millis(2000)
        );
        // 2000 * 2^1 = 4000ms
        assert_eq!(
            config.delay_for_attempt(1),
            std::time::Duration::from_millis(4000)
        );
        // 2000 * 2^2 = 8000ms
        assert_eq!(
            config.delay_for_attempt(2),
            std::time::Duration::from_millis(8000)
        );
        // 2000 * 2^3 = 16000ms
        assert_eq!(
            config.delay_for_attempt(3),
            std::time::Duration::from_millis(16000)
        );
    }

    #[test]
    fn test_retry_config_exponential_backoff_capped() {
        let config = RetryConfig::default();
        // 2000 * 2^10 = 2,048,000ms > 30,000ms cap
        assert_eq!(
            config.delay_for_attempt(10),
            std::time::Duration::from_millis(30000)
        );
    }

    #[test]
    fn test_retry_config_legacy_fallback() {
        let config = RetryConfig {
            initial_delay_ms: 0, // Disable exponential backoff
            ..Default::default()
        };
        // Falls back to retry_delays array
        assert_eq!(
            config.delay_for_attempt(0),
            std::time::Duration::from_secs(1)
        );
        assert_eq!(
            config.delay_for_attempt(1),
            std::time::Duration::from_secs(2)
        );
    }

    #[test]
    fn test_retry_config_serde_roundtrip() {
        let config = RetryConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_retries, config.max_retries);
        assert_eq!(deserialized.initial_delay_ms, config.initial_delay_ms);
        assert_eq!(deserialized.backoff_factor, config.backoff_factor);
        assert_eq!(deserialized.max_delay_ms, config.max_delay_ms);
    }

    // --- parse_retry_after tests ---

    #[test]
    fn test_parse_retry_after_ms_takes_precedence() {
        let result = parse_retry_after(Some("10"), Some("500"));
        assert_eq!(result, Some(std::time::Duration::from_millis(500)));
    }

    #[test]
    fn test_parse_retry_after_seconds() {
        let result = parse_retry_after(Some("5"), None);
        assert_eq!(result, Some(std::time::Duration::from_secs(5)));
    }

    #[test]
    fn test_parse_retry_after_float_seconds() {
        let result = parse_retry_after(Some("2.5"), None);
        assert_eq!(result, Some(std::time::Duration::from_secs_f64(2.5)));
    }

    #[test]
    fn test_parse_retry_after_invalid() {
        let result = parse_retry_after(Some("invalid"), None);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_retry_after_none() {
        let result = parse_retry_after(None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_retry_after_zero() {
        let result = parse_retry_after(Some("0"), None);
        assert!(result.is_none()); // 0 seconds is not a valid delay
    }

    // --- classify_retryable_error tests ---

    #[test]
    fn test_classify_429_rate_limited() {
        let body = serde_json::json!({"error": {"message": "rate_limit exceeded"}});
        let result = classify_retryable_error(Some(429), Some(&body));
        assert_eq!(result, Some("Rate Limited".to_string()));
    }

    #[test]
    fn test_classify_429_generic() {
        let result = classify_retryable_error(Some(429), None);
        assert_eq!(result, Some("Rate Limited".to_string()));
    }

    #[test]
    fn test_classify_503_overloaded() {
        let body = serde_json::json!({"error": {"message": "Server overloaded"}});
        let result = classify_retryable_error(Some(503), Some(&body));
        assert_eq!(result, Some("Provider is overloaded".to_string()));
    }

    #[test]
    fn test_classify_503_generic() {
        let result = classify_retryable_error(Some(503), None);
        assert_eq!(result, Some("Service Unavailable".to_string()));
    }

    #[test]
    fn test_classify_500() {
        let result = classify_retryable_error(Some(500), None);
        assert_eq!(result, Some("Internal Server Error".to_string()));
    }

    #[test]
    fn test_classify_502() {
        let result = classify_retryable_error(Some(502), None);
        assert_eq!(result, Some("Bad Gateway".to_string()));
    }

    #[test]
    fn test_classify_504() {
        let result = classify_retryable_error(Some(504), None);
        assert_eq!(result, Some("Gateway Timeout".to_string()));
    }

    #[test]
    fn test_classify_529_overloaded() {
        let result = classify_retryable_error(Some(529), None);
        assert_eq!(result, Some("Provider is overloaded".to_string()));
    }

    #[test]
    fn test_classify_404_not_retryable() {
        let result = classify_retryable_error(Some(404), None);
        assert!(result.is_none());
    }

    #[test]
    fn test_classify_body_overloaded_no_status() {
        let body = serde_json::json!({"error": {"message": "Server is overloaded"}});
        let result = classify_retryable_error(Some(200), Some(&body));
        assert_eq!(result, Some("Provider is overloaded".to_string()));
    }

    // --- extract_error_message tests ---

    #[test]
    fn test_extract_openai_error() {
        let body = serde_json::json!({"error": {"message": "Invalid API key", "type": "auth_error"}});
        assert_eq!(
            extract_error_message(&body),
            Some("Invalid API key".to_string())
        );
    }

    #[test]
    fn test_extract_anthropic_error() {
        let body = serde_json::json!({"type": "error", "error": {"type": "rate_limit_error", "message": "Rate limited"}});
        assert_eq!(
            extract_error_message(&body),
            Some("Rate limited".to_string())
        );
    }

    #[test]
    fn test_extract_generic_message() {
        let body = serde_json::json!({"message": "Something went wrong"});
        assert_eq!(
            extract_error_message(&body),
            Some("Something went wrong".to_string())
        );
    }

    #[test]
    fn test_extract_no_message() {
        let body = serde_json::json!({"status": "error"});
        assert!(extract_error_message(&body).is_none());
    }
}
