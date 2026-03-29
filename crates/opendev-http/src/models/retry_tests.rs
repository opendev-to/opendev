use super::*;

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
    // Delays include ±25% jitter, so check ranges
    let d0 = config.delay_for_attempt(0).as_millis() as u64;
    assert!(
        d0 >= 1500 && d0 <= 2500,
        "attempt 0: {d0}ms not in [1500, 2500]"
    );

    let d1 = config.delay_for_attempt(1).as_millis() as u64;
    assert!(
        d1 >= 3000 && d1 <= 5000,
        "attempt 1: {d1}ms not in [3000, 5000]"
    );

    let d2 = config.delay_for_attempt(2).as_millis() as u64;
    assert!(
        d2 >= 6000 && d2 <= 10000,
        "attempt 2: {d2}ms not in [6000, 10000]"
    );

    let d3 = config.delay_for_attempt(3).as_millis() as u64;
    assert!(
        d3 >= 12000 && d3 <= 20000,
        "attempt 3: {d3}ms not in [12000, 20000]"
    );
}

#[test]
fn test_retry_config_exponential_backoff_capped() {
    let config = RetryConfig::default();
    // 2000 * 2^10 = 2,048,000ms > 30,000ms cap, then ±25% jitter
    let d = config.delay_for_attempt(10).as_millis() as u64;
    assert!(
        d >= 22500 && d <= 37500,
        "attempt 10: {d}ms not in [22500, 37500]"
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
    let body =
        serde_json::json!({"error": {"message": "Invalid API key", "type": "auth_error"}});
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
