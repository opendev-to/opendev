use super::*;

#[test]
fn test_initial_state_is_closed() {
    let cb = CircuitBreaker::with_defaults("test");
    assert_eq!(cb.state(), CircuitState::Closed);
    assert!(cb.check().is_ok());
    assert_eq!(cb.failure_count(), 0);
}

#[test]
fn test_opens_after_threshold() {
    let cb = CircuitBreaker::new("test", 3, Duration::from_secs(30));

    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Closed);
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Closed);
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);
    assert!(cb.check().is_err());
}

#[test]
fn test_success_resets() {
    let cb = CircuitBreaker::new("test", 3, Duration::from_secs(30));

    cb.record_failure();
    cb.record_failure();
    cb.record_success();
    assert_eq!(cb.failure_count(), 0);
    assert_eq!(cb.state(), CircuitState::Closed);
}

#[test]
fn test_half_open_after_cooldown() {
    let cb = CircuitBreaker::new("test", 2, Duration::from_millis(0));

    cb.record_failure();
    cb.record_failure();

    // With a 0ms cooldown, it should immediately transition to half-open.
    assert_eq!(cb.state(), CircuitState::HalfOpen);
    assert!(cb.check().is_ok());
}

#[test]
fn test_reset() {
    let cb = CircuitBreaker::new("test", 2, Duration::from_secs(60));

    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);

    cb.reset();
    assert_eq!(cb.state(), CircuitState::Closed);
    assert_eq!(cb.failure_count(), 0);
}

#[test]
fn test_debug_format() {
    let cb = CircuitBreaker::with_defaults("openai");
    let debug = format!("{:?}", cb);
    assert!(debug.contains("openai"));
    assert!(debug.contains("Closed"));
}

#[test]
fn test_open_circuit_error_message() {
    let cb = CircuitBreaker::new("anthropic", 1, Duration::from_secs(60));
    cb.record_failure();

    let err = cb.check().unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("anthropic"));
    assert!(msg.contains("Circuit breaker open"));
}

#[test]
fn test_partial_failures_dont_open() {
    let cb = CircuitBreaker::new("test", 5, Duration::from_secs(30));

    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    // 3 failures, threshold is 5 — should still be closed
    assert_eq!(cb.state(), CircuitState::Closed);
    assert!(cb.check().is_ok());
}

#[test]
fn test_success_after_half_open_closes_circuit() {
    let cb = CircuitBreaker::new("test", 2, Duration::from_millis(0));

    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::HalfOpen);

    // Probe succeeds
    cb.record_success();
    assert_eq!(cb.state(), CircuitState::Closed);
    assert_eq!(cb.failure_count(), 0);
}

#[test]
fn test_failure_after_half_open_reopens() {
    let cb = CircuitBreaker::new("test", 2, Duration::from_millis(0));

    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::HalfOpen);

    // Probe fails — counter goes to 3, which is >= threshold of 2
    cb.record_failure();
    // Since cooldown is 0ms, it'll be HalfOpen again immediately
    // but the failure count is 3, confirming the circuit was re-triggered
    assert!(cb.failure_count() >= cb.threshold);
}

// --- #57: CircuitBreakerConfig tests ---

#[test]
fn test_circuit_breaker_config_default() {
    let config = CircuitBreakerConfig::default();
    assert_eq!(config.failure_threshold, 5);
    assert_eq!(config.reset_timeout_secs, 30);
    assert_eq!(config.probe_interval_secs, 30);
}

#[test]
fn test_circuit_breaker_config_serde_roundtrip() {
    let config = CircuitBreakerConfig {
        failure_threshold: 10,
        reset_timeout_secs: 60,
        probe_interval_secs: 15,
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: CircuitBreakerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.failure_threshold, 10);
    assert_eq!(deserialized.reset_timeout_secs, 60);
    assert_eq!(deserialized.probe_interval_secs, 15);
}

#[test]
fn test_circuit_breaker_from_config() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        reset_timeout_secs: 10,
        probe_interval_secs: 5,
    };
    let cb = CircuitBreaker::from_config("test-provider", &config);

    assert_eq!(cb.state(), CircuitState::Closed);
    assert_eq!(cb.threshold, 3);
    assert_eq!(cb.cooldown, Duration::from_secs(10));

    // Open after 3 failures
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Closed);
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);
}

#[test]
fn test_circuit_breaker_config_from_json() {
    let json =
        r#"{"failure_threshold": 7, "reset_timeout_secs": 45, "probe_interval_secs": 10}"#;
    let config: CircuitBreakerConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.failure_threshold, 7);
    assert_eq!(config.reset_timeout_secs, 45);
    assert_eq!(config.probe_interval_secs, 10);
}
