//! HTTP client with retry logic and cancellation support.

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::models::{HttpError, HttpResult, RetryConfig};

/// Timeout configuration for HTTP requests.
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    pub connect: Duration,
    pub read: Duration,
    pub write: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            read: Duration::from_secs(300),
            write: Duration::from_secs(10),
        }
    }
}

/// Async HTTP client with retry and cancellation support.
///
/// Wraps reqwest with:
/// - Exponential backoff retries on 429/503
/// - Respect for `Retry-After` headers
/// - Cancellation via `CancellationToken` (checked between retries and via `tokio::select!`)
pub struct HttpClient {
    client: reqwest::Client,
    api_url: String,
    retry_config: RetryConfig,
    circuit_breaker: Option<std::sync::Arc<crate::circuit_breaker::CircuitBreaker>>,
}

impl HttpClient {
    /// Create a new HTTP client.
    pub fn new(
        api_url: impl Into<String>,
        headers: HeaderMap,
        timeout: Option<TimeoutConfig>,
    ) -> Result<Self, HttpError> {
        let timeout = timeout.unwrap_or_default();
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(timeout.connect)
            .timeout(timeout.read)
            .build()?;

        Ok(Self {
            client,
            api_url: api_url.into(),
            retry_config: RetryConfig::default(),
            circuit_breaker: None,
        })
    }

    /// Create a client with custom retry configuration.
    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Attach a circuit breaker to this client.
    ///
    /// When set, every request is gated by the circuit breaker. Successful
    /// responses close the circuit; failures (transport-level or 5xx) open it.
    pub fn with_circuit_breaker(
        mut self,
        cb: std::sync::Arc<crate::circuit_breaker::CircuitBreaker>,
    ) -> Self {
        self.circuit_breaker = Some(cb);
        self
    }

    /// POST JSON with retry logic and optional cancellation.
    ///
    /// On 429/503 responses, retries with exponential backoff. Respects
    /// `Retry-After` headers. Checks the cancellation token between attempts
    /// and races it against each request via `tokio::select!`.
    ///
    /// When a circuit breaker is attached, requests are rejected immediately
    /// if the circuit is open.
    pub async fn post_json(
        &self,
        payload: &serde_json::Value,
        cancel: Option<&CancellationToken>,
    ) -> Result<HttpResult, HttpError> {
        // Check circuit breaker before attempting any request.
        if let Some(cb) = &self.circuit_breaker {
            cb.check()?;
        }

        let mut last_result: Option<HttpResult> = None;

        for attempt in 0..=self.retry_config.max_retries {
            // Check cancellation before each attempt
            if let Some(token) = cancel
                && token.is_cancelled()
            {
                return Ok(HttpResult::interrupted());
            }

            let result = self.execute_request(payload, cancel).await;

            match result {
                Ok(hr) if hr.success => {
                    // Check if status is retryable (429/503 with a body)
                    if let Some(status) = hr.status
                        && self.retry_config.is_retryable_status(status)
                    {
                        let delay = self.get_retry_delay(hr.retry_after.as_deref(), hr.retry_after_ms.as_deref(), attempt);
                        last_result = Some(hr);
                        if attempt < self.retry_config.max_retries {
                            warn!(
                                status,
                                attempt = attempt + 1,
                                max = self.retry_config.max_retries,
                                "Retryable HTTP status, backing off for {:.1}s",
                                delay.as_secs_f64()
                            );
                            self.interruptible_sleep(delay, cancel).await?;
                            continue;
                        }
                        warn!(
                            status,
                            "Exhausted {} retries", self.retry_config.max_retries
                        );
                        self.cb_record_failure();
                        return Ok(last_result.unwrap_or_else(|| {
                            HttpResult::fail("Unexpected retry exhaustion", false)
                        }));
                    }
                    self.cb_record_success();
                    return Ok(hr);
                }
                Ok(hr) if hr.retryable => {
                    let retry_after = hr.retry_after.clone();
                    let retry_after_ms = hr.retry_after_ms.clone();
                    last_result = Some(hr);
                    if attempt < self.retry_config.max_retries {
                        let delay = self.get_retry_delay(retry_after.as_deref(), retry_after_ms.as_deref(), attempt);
                        warn!(
                            error = last_result.as_ref().and_then(|r| r.error.as_deref()),
                            attempt = attempt + 1,
                            max = self.retry_config.max_retries,
                            "Retryable error, backing off for {:.1}s",
                            delay.as_secs_f64()
                        );
                        self.interruptible_sleep(delay, cancel).await?;
                        continue;
                    }
                    warn!("Exhausted {} retries", self.retry_config.max_retries);
                    self.cb_record_failure();
                    return Ok(last_result.unwrap_or_else(|| {
                        HttpResult::fail("Unexpected retry exhaustion", false)
                    }));
                }
                Ok(hr) => {
                    if hr.success {
                        self.cb_record_success();
                    } else {
                        self.cb_record_failure();
                    }
                    return Ok(hr);
                }
                Err(e) => {
                    self.cb_record_failure();
                    return Err(e);
                }
            }
        }

        self.cb_record_failure();
        Ok(last_result.unwrap_or_else(|| HttpResult::fail("Unexpected retry exhaustion", false)))
    }

    /// Record a success on the circuit breaker (if attached).
    fn cb_record_success(&self) {
        if let Some(cb) = &self.circuit_breaker {
            cb.record_success();
        }
    }

    /// Record a failure on the circuit breaker (if attached).
    fn cb_record_failure(&self) {
        if let Some(cb) = &self.circuit_breaker {
            cb.record_failure();
        }
    }

    /// Execute a single POST request, racing against cancellation.
    ///
    /// Each request is tagged with a unique `X-Request-Id` header and
    /// logged via a tracing span for end-to-end observability.
    async fn execute_request(
        &self,
        payload: &serde_json::Value,
        cancel: Option<&CancellationToken>,
    ) -> Result<HttpResult, HttpError> {
        let request_id = Uuid::new_v4().to_string();
        debug!(request_id = %request_id, api_url = %self.api_url, "Sending LLM request");

        let request = self
            .client
            .post(&self.api_url)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .header(
                HeaderName::from_static("x-request-id"),
                HeaderValue::from_str(&request_id)
                    .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
            )
            .json(payload)
            .send();

        let response = match cancel {
            Some(token) => {
                tokio::select! {
                    resp = request => resp,
                    _ = token.cancelled() => {
                        return Ok(HttpResult::interrupted()
                            .with_request_id(request_id));
                    }
                }
            }
            None => request.await,
        };

        match response {
            Ok(resp) => {
                let status = resp.status().as_u16();
                debug!(request_id = %request_id, status, "LLM response received");
                if self.retry_config.is_retryable_status(status) {
                    // Extract Retry-After and retry-after-ms headers
                    let retry_after = resp
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .map(String::from);
                    let retry_after_ms = resp
                        .headers()
                        .get("retry-after-ms")
                        .and_then(|v| v.to_str().ok())
                        .map(String::from);
                    let body = resp.json::<serde_json::Value>().await.ok();
                    let mut result =
                        HttpResult::retryable_status(status, body, retry_after)
                            .with_request_id(request_id);
                    result.retry_after_ms = retry_after_ms;
                    return Ok(result);
                }
                let body = resp.json::<serde_json::Value>().await?;
                if status >= 400 {
                    let error_msg = body
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("HTTP {status}"));
                    warn!(request_id = %request_id, status, error = %error_msg, "LLM request failed");
                    return Ok(HttpResult {
                        success: false,
                        status: Some(status),
                        body: Some(body),
                        error: Some(format!("[request_id={}] {}", request_id, error_msg)),
                        interrupted: false,
                        retryable: false,
                        request_id: Some(request_id),
                        retry_after: None,
                        retry_after_ms: None,
                    });
                }
                Ok(HttpResult::ok(status, body).with_request_id(request_id))
            }
            Err(e) if is_retryable_error(&e) => {
                warn!(request_id = %request_id, error = %e, "LLM request retryable error");
                Ok(
                    HttpResult::fail(format!("[request_id={}] {}", request_id, e), true)
                        .with_request_id(request_id),
                )
            }
            Err(e) => {
                warn!(request_id = %request_id, error = %e, "LLM request error");
                Ok(
                    HttpResult::fail(format!("[request_id={}] {}", request_id, e), false)
                        .with_request_id(request_id),
                )
            }
        }
    }

    /// Determine retry delay from Retry-After/retry-after-ms headers or default backoff.
    fn get_retry_delay(
        &self,
        retry_after: Option<&str>,
        retry_after_ms: Option<&str>,
        attempt: u32,
    ) -> Duration {
        if let Some(parsed) =
            crate::models::parse_retry_after(retry_after, retry_after_ms)
        {
            // Cap server-requested delay at max_delay_ms
            let max = Duration::from_millis(self.retry_config.max_delay_ms);
            return parsed.min(max);
        }
        self.retry_config.delay_for_attempt(attempt)
    }

    /// Sleep that can be interrupted by cancellation.
    async fn interruptible_sleep(
        &self,
        duration: Duration,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), HttpError> {
        match cancel {
            Some(token) => {
                tokio::select! {
                    _ = tokio::time::sleep(duration) => Ok(()),
                    _ = token.cancelled() => Err(HttpError::Interrupted),
                }
            }
            None => {
                tokio::time::sleep(duration).await;
                Ok(())
            }
        }
    }

    /// Get the configured API URL.
    pub fn api_url(&self) -> &str {
        &self.api_url
    }
}

impl std::fmt::Debug for HttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("HttpClient");
        s.field("api_url", &self.api_url)
            .field("retry_config", &self.retry_config);
        if let Some(cb) = &self.circuit_breaker {
            s.field("circuit_breaker", cb);
        }
        s.finish()
    }
}

/// Check if a reqwest error is transient and worth retrying.
fn is_retryable_error(err: &reqwest::Error) -> bool {
    err.is_connect() || err.is_timeout()
}

#[cfg(test)]
mod tests {
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
        let delay = client.get_retry_delay(None, None, 0);
        assert_eq!(delay, Duration::from_secs(2)); // 2000ms initial
        let delay = client.get_retry_delay(Some("invalid"), None, 1);
        assert_eq!(delay, Duration::from_secs(4)); // 2000 * 2^1 = 4000ms
    }

    #[test]
    fn test_get_retry_delay_capped() {
        let client = HttpClient::new("https://example.com", HeaderMap::new(), None).unwrap();
        // Attempt 10: 2000 * 2^10 = 2,048,000ms, but capped at 30,000ms
        let delay = client.get_retry_delay(None, None, 10);
        assert_eq!(delay, Duration::from_millis(30000));
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
}
