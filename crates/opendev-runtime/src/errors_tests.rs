use super::*;

#[test]
fn test_classify_context_overflow_anthropic() {
    let err = classify_api_error("prompt is too long: 250000 tokens", None, Some("anthropic"));
    assert_eq!(err.category, ErrorCategory::ContextOverflow);
    assert!(err.is_retryable);
    assert!(err.should_compact());
}

#[test]
fn test_classify_context_overflow_openai() {
    let err = classify_api_error(
        "This model's maximum context length is 128000 tokens",
        None,
        Some("openai"),
    );
    assert_eq!(err.category, ErrorCategory::ContextOverflow);
    assert!(err.is_retryable);
}

#[test]
fn test_classify_context_overflow_google() {
    let err = classify_api_error("GenerateContentRequest is too large", None, Some("google"));
    assert_eq!(err.category, ErrorCategory::ContextOverflow);
}

#[test]
fn test_classify_rate_limit() {
    let err = classify_api_error("Rate limit exceeded", Some(429), Some("openai"));
    assert_eq!(err.category, ErrorCategory::RateLimit);
    assert!(err.is_retryable);
}

#[test]
fn test_classify_rate_limit_with_retry_after() {
    let err = classify_api_error(
        "Too many requests. Retry-After: 30",
        Some(429),
        Some("anthropic"),
    );
    assert_eq!(err.category, ErrorCategory::RateLimit);
    assert_eq!(err.retry_after, Some(30.0));
}

#[test]
fn test_classify_auth_by_status_code() {
    let err = classify_api_error("Forbidden", Some(401), None);
    assert_eq!(err.category, ErrorCategory::Auth);
    assert!(!err.is_retryable);
}

#[test]
fn test_classify_auth_by_pattern() {
    let err = classify_api_error("Invalid API key provided", Some(400), Some("openai"));
    assert_eq!(err.category, ErrorCategory::Auth);
    assert!(!err.is_retryable);
}

#[test]
fn test_classify_gateway_html() {
    let err = classify_api_error(
        "<!doctype html><html>502 Bad Gateway</html>",
        Some(502),
        None,
    );
    assert_eq!(err.category, ErrorCategory::Gateway);
    assert!(err.is_retryable);
    assert!(err.original_error.is_some());
}

#[test]
fn test_classify_gateway_401_html() {
    let err = classify_api_error("<html>Unauthorized</html>", Some(401), None);
    assert_eq!(err.category, ErrorCategory::Gateway);
    assert!(err.message.contains("Authentication failed at gateway"));
}

#[test]
fn test_classify_generic_500() {
    let err = classify_api_error("Internal server error", Some(500), None);
    assert_eq!(err.category, ErrorCategory::Api);
    assert!(err.is_retryable);
}

#[test]
fn test_classify_unknown() {
    let err = classify_api_error("Something went wrong", None, None);
    assert_eq!(err.category, ErrorCategory::Unknown);
    assert!(!err.is_retryable);
}

#[test]
fn test_structured_error_display() {
    let err = StructuredError::api("test error", Some(500));
    let display = format!("{}", err);
    assert!(display.contains("E5002_API_ERROR"));
    assert!(display.contains("test error"));
}

#[test]
fn test_structured_error_serialization() {
    let err = StructuredError::context_overflow(
        "too many tokens",
        Some("anthropic".to_string()),
        Some(200000),
        Some(128000),
    );
    let json = serde_json::to_string(&err).unwrap();
    let deserialized: StructuredError = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.category, ErrorCategory::ContextOverflow);
    assert_eq!(deserialized.token_count, Some(200000));
    assert_eq!(deserialized.token_limit, Some(128000));
}

// --- #55: error_code and to_json tests ---

#[test]
fn test_error_code_rate_limit() {
    let err = StructuredError::rate_limit("rate limited", None, Some(30.0));
    assert_eq!(err.error_code(), "E1001_RATE_LIMIT");
}

#[test]
fn test_error_code_context_overflow() {
    let err = StructuredError::context_overflow("overflow", None, None, None);
    assert_eq!(err.error_code(), "E3001_CONTEXT_OVERFLOW");
}

#[test]
fn test_error_code_auth() {
    let err = StructuredError::auth("bad key", Some(401), None);
    assert_eq!(err.error_code(), "E4001_AUTH_FAILED");
}

#[test]
fn test_error_code_gateway() {
    let err = StructuredError::gateway("bad gw", Some(502), None, None);
    assert_eq!(err.error_code(), "E5001_GATEWAY_ERROR");
}

#[test]
fn test_error_code_api() {
    let err = StructuredError::api("server error", Some(500));
    assert_eq!(err.error_code(), "E5002_API_ERROR");
}

#[test]
fn test_error_code_unknown() {
    let err = StructuredError::api("mystery", None);
    assert_eq!(err.error_code(), "E9001_UNKNOWN");
}

#[test]
fn test_to_json_includes_error_code() {
    let err = StructuredError::rate_limit("rate limited", Some("openai".into()), Some(30.0));
    let json = err.to_json();
    assert_eq!(json["error_code"], "E1001_RATE_LIMIT");
    assert_eq!(json["category"], "rate_limit");
    assert_eq!(json["message"], "rate limited");
    assert_eq!(json["is_retryable"], true);
    assert_eq!(json["provider"], "openai");
    assert_eq!(json["retry_after"], 30.0);
    // recovery_strategy should be present
    assert!(json["recovery_strategy"]["type"].is_string());
}

#[test]
fn test_to_json_context_overflow_includes_tokens() {
    let err = StructuredError::context_overflow(
        "overflow",
        Some("anthropic".into()),
        Some(200000),
        Some(128000),
    );
    let json = err.to_json();
    assert_eq!(json["error_code"], "E3001_CONTEXT_OVERFLOW");
    assert_eq!(json["token_count"], 200000);
    assert_eq!(json["token_limit"], 128000);
}

#[test]
fn test_to_json_omits_none_fields() {
    let err = StructuredError::api("error", Some(500));
    let json = err.to_json();
    assert!(json.get("provider").is_none());
    assert!(json.get("token_count").is_none());
    assert!(json.get("retry_after").is_none());
}

#[test]
fn test_display_includes_error_code() {
    let err = StructuredError::api("test error", Some(500));
    let display = format!("{}", err);
    assert!(display.contains("E5002_API_ERROR"));
    assert!(display.contains("test error"));
}

// --- #56: RecoveryStrategy tests ---

#[test]
fn test_recovery_strategy_rate_limit_with_retry_after() {
    let err = StructuredError::rate_limit("rate limited", None, Some(10.0));
    match err.recovery_strategy() {
        RecoveryStrategy::Retry {
            delay_ms,
            max_attempts,
        } => {
            assert_eq!(delay_ms, 10000);
            assert_eq!(max_attempts, 3);
        }
        other => panic!("Expected Retry, got {:?}", other),
    }
}

#[test]
fn test_recovery_strategy_rate_limit_default_delay() {
    let err = StructuredError::rate_limit("rate limited", None, None);
    match err.recovery_strategy() {
        RecoveryStrategy::Retry {
            delay_ms,
            max_attempts,
        } => {
            assert_eq!(delay_ms, 5000);
            assert_eq!(max_attempts, 3);
        }
        other => panic!("Expected Retry, got {:?}", other),
    }
}

#[test]
fn test_recovery_strategy_context_overflow() {
    let err = StructuredError::context_overflow("overflow", None, None, None);
    assert_eq!(err.recovery_strategy(), RecoveryStrategy::ReduceContext);
}

#[test]
fn test_recovery_strategy_auth() {
    let err = StructuredError::auth("bad key", Some(401), None);
    match err.recovery_strategy() {
        RecoveryStrategy::UserIntervention { message } => {
            assert!(message.contains("API key"));
        }
        other => panic!("Expected UserIntervention, got {:?}", other),
    }
}

#[test]
fn test_recovery_strategy_retryable_api() {
    let err = StructuredError::api("server error", Some(500));
    match err.recovery_strategy() {
        RecoveryStrategy::Retry { .. } => {}
        other => panic!("Expected Retry, got {:?}", other),
    }
}

#[test]
fn test_recovery_strategy_non_retryable_api() {
    let err = StructuredError::api("bad request", Some(400));
    match err.recovery_strategy() {
        RecoveryStrategy::FallbackModel { model } => {
            assert_eq!(model, "default");
        }
        other => panic!("Expected FallbackModel, got {:?}", other),
    }
}

#[test]
fn test_recovery_strategy_gateway() {
    let err = StructuredError::gateway("bad gw", Some(502), None, None);
    match err.recovery_strategy() {
        RecoveryStrategy::Retry {
            delay_ms,
            max_attempts,
        } => {
            assert_eq!(delay_ms, 3000);
            assert_eq!(max_attempts, 3);
        }
        other => panic!("Expected Retry, got {:?}", other),
    }
}

#[test]
fn test_recovery_strategy_serialization() {
    let strategy = RecoveryStrategy::Retry {
        delay_ms: 5000,
        max_attempts: 3,
    };
    let json = strategy.to_json();
    assert_eq!(json["type"], "retry");
    assert_eq!(json["delay_ms"], 5000);
    assert_eq!(json["max_attempts"], 3);
}

#[test]
fn test_recovery_strategy_fallback_serialization() {
    let strategy = RecoveryStrategy::FallbackModel {
        model: "gpt-4".into(),
    };
    let json = strategy.to_json();
    assert_eq!(json["type"], "fallback_model");
    assert_eq!(json["model"], "gpt-4");
}

#[test]
fn test_recovery_strategy_reduce_context_serialization() {
    let strategy = RecoveryStrategy::ReduceContext;
    let json = strategy.to_json();
    assert_eq!(json["type"], "reduce_context");
}
