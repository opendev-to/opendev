//! Retry configuration, backoff logic, and error classification.

use serde::{Deserialize, Serialize};

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
            let delay_ms = self.initial_delay_ms as f64 * self.backoff_factor.powi(attempt as i32);
            let capped_ms = delay_ms.min(self.max_delay_ms as f64);
            // Add ±25% random jitter to avoid thundering herd when parallel
            // agents all retry at the same backoff intervals
            let jitter_factor = 0.75 + fastrand::f64() * 0.5; // [0.75, 1.25]
            let final_ms = (capped_ms * jitter_factor) as u64;
            return std::time::Duration::from_millis(final_ms);
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
pub(super) fn extract_error_message(body: &serde_json::Value) -> Option<String> {
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
#[path = "retry_tests.rs"]
mod tests;
