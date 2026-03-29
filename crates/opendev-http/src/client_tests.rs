use super::*;

#[test]
fn test_timeout_config_default() {
    let tc = TimeoutConfig::default();
    assert_eq!(tc.connect, Duration::from_secs(10));
    assert_eq!(tc.read, Duration::from_secs(300));
    assert_eq!(tc.write, Duration::from_secs(10));
}

#[test]
fn test_http_client_debug() {
    let client =
        HttpClient::new("https://api.example.com/v1/chat", HeaderMap::new(), None).unwrap();
    let debug = format!("{:?}", client);
    assert!(debug.contains("api.example.com"));
}

#[test]
fn test_get_retry_delay_with_header() {
    let client = HttpClient::new("https://example.com", HeaderMap::new(), None).unwrap();
    let delay = client.get_retry_delay(Some("5.0"), None, 0);
    assert_eq!(delay, Duration::from_secs(5));
}

#[test]
fn test_get_retry_delay_with_ms_header() {
    let client = HttpClient::new("https://example.com", HeaderMap::new(), None).unwrap();
    // retry-after-ms takes precedence over retry-after
    let delay = client.get_retry_delay(Some("10"), Some("500"), 0);
    assert_eq!(delay, Duration::from_millis(500));
}

#[test]
fn test_get_retry_delay_fallback() {
    let client = HttpClient::new("https://example.com", HeaderMap::new(), None).unwrap();
    // Delays include ±25% jitter
    let d0 = client.get_retry_delay(None, None, 0).as_millis() as u64;
    assert!(
        d0 >= 1500 && d0 <= 2500,
        "attempt 0: {d0}ms not in [1500, 2500]"
    );
    let d1 = client.get_retry_delay(Some("invalid"), None, 1).as_millis() as u64;
    assert!(
        d1 >= 3000 && d1 <= 5000,
        "attempt 1: {d1}ms not in [3000, 5000]"
    );
}

#[test]
fn test_get_retry_delay_capped() {
    let client = HttpClient::new("https://example.com", HeaderMap::new(), None).unwrap();
    // Attempt 10: 2000 * 2^10 capped at 30,000ms, then ±25% jitter
    let d = client.get_retry_delay(None, None, 10).as_millis() as u64;
    assert!(
        d >= 22500 && d <= 37500,
        "attempt 10: {d}ms not in [22500, 37500]"
    );
}

#[tokio::test]
async fn test_cancellation_before_request() {
    let client = HttpClient::new("https://example.com", HeaderMap::new(), None).unwrap();
    let token = CancellationToken::new();
    token.cancel();

    let result = client
        .post_json(&serde_json::json!({}), Some(&token))
        .await
        .unwrap();
    assert!(result.interrupted);
    assert!(!result.success);
}

#[tokio::test]
async fn test_interruptible_sleep_cancel() {
    let client = HttpClient::new("https://example.com", HeaderMap::new(), None).unwrap();
    let token = CancellationToken::new();
    token.cancel();

    let err = client
        .interruptible_sleep(Duration::from_secs(60), Some(&token))
        .await;
    assert!(matches!(err, Err(HttpError::Interrupted)));
}

#[tokio::test]
async fn test_interruptible_sleep_completes() {
    let client = HttpClient::new("https://example.com", HeaderMap::new(), None).unwrap();
    let result = client
        .interruptible_sleep(Duration::from_millis(10), None)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_circuit_breaker_rejects_when_open() {
    let cb = std::sync::Arc::new(crate::circuit_breaker::CircuitBreaker::new(
        "test",
        2,
        Duration::from_secs(60),
    ));
    let client = HttpClient::new("https://example.com", HeaderMap::new(), None)
        .unwrap()
        .with_circuit_breaker(cb.clone());

    // Open the circuit
    cb.record_failure();
    cb.record_failure();

    let result = client.post_json(&serde_json::json!({}), None).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Circuit breaker open"));
}

// --- #60: Request ID tracing tests ---

#[test]
fn test_http_result_with_request_id() {
    let result = HttpResult::ok(200, serde_json::json!({})).with_request_id("test-uuid-1234");
    assert_eq!(result.request_id.as_deref(), Some("test-uuid-1234"));
}

#[test]
fn test_http_result_fail_with_request_id() {
    let result = HttpResult::fail("error", true).with_request_id("req-5678");
    assert_eq!(result.request_id.as_deref(), Some("req-5678"));
}

#[test]
fn test_http_result_interrupted_with_request_id() {
    let result = HttpResult::interrupted().with_request_id("req-cancel");
    assert_eq!(result.request_id.as_deref(), Some("req-cancel"));
    assert!(result.interrupted);
}

#[test]
fn test_http_result_default_no_request_id() {
    let result = HttpResult::ok(200, serde_json::json!({}));
    assert!(result.request_id.is_none());
}

#[test]
fn test_http_client_debug_with_circuit_breaker() {
    let cb = std::sync::Arc::new(crate::circuit_breaker::CircuitBreaker::with_defaults(
        "openai",
    ));
    let client = HttpClient::new("https://api.example.com/v1/chat", HeaderMap::new(), None)
        .unwrap()
        .with_circuit_breaker(cb);
    let debug = format!("{:?}", client);
    assert!(debug.contains("circuit_breaker"));
    assert!(debug.contains("openai"));
}
