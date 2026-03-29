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
