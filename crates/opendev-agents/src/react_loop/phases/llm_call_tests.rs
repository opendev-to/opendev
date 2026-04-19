//! Unit tests for `llm_call` retry-backoff logic.

use std::time::Duration;

use super::{RETRY_FALLBACK_BACKOFF, RETRY_MAX_BACKOFF, parse_retry_hint, retry_backoff_for};

#[test]
fn parses_canonical_circuit_breaker_message() {
    let msg = "Circuit breaker open for provider 'anthropic'. \
               Too many consecutive failures (9). \
               Will retry in 27s.";
    assert_eq!(parse_retry_hint(msg), Some(Duration::from_secs(27)));
}

#[test]
fn parses_with_extra_surrounding_context() {
    let msg = "[request_id=abc] Circuit open. Will retry in 5s. (jittered)";
    assert_eq!(parse_retry_hint(msg), Some(Duration::from_secs(5)));
}

#[test]
fn returns_none_when_phrase_absent() {
    assert_eq!(parse_retry_hint("HTTP 500 internal server error"), None);
    assert_eq!(parse_retry_hint(""), None);
}

#[test]
fn returns_none_when_seconds_unparseable() {
    assert_eq!(parse_retry_hint("Will retry in soons."), None);
    assert_eq!(parse_retry_hint("Will retry in -1s."), None);
}

#[test]
fn backoff_uses_parsed_hint_when_present() {
    let msg = "Circuit open. Will retry in 3s.";
    assert_eq!(retry_backoff_for(msg), Duration::from_secs(3));
}

#[test]
fn backoff_caps_unreasonably_large_hints() {
    let msg = "Will retry in 999999s.";
    assert_eq!(retry_backoff_for(msg), RETRY_MAX_BACKOFF);
}

#[test]
fn backoff_falls_back_when_no_hint() {
    assert_eq!(retry_backoff_for("HTTP 500"), RETRY_FALLBACK_BACKOFF);
    assert_eq!(retry_backoff_for(""), RETRY_FALLBACK_BACKOFF);
}

#[test]
fn parsed_zero_seconds_clamps_up_to_fallback() {
    // Reproduces the half-open boundary case observed in the field:
    // the circuit breaker reports `remaining_secs=0` the moment its
    // cooldown expires. A naive parse would yield a zero-second sleep,
    // letting the loop burst-retry until the breaker fully opens
    // again. The clamp must lift parsed=0 up to the fallback.
    let msg = "Circuit breaker open … Will retry in 0s.";
    assert_eq!(parse_retry_hint(msg), Some(Duration::ZERO));
    assert_eq!(retry_backoff_for(msg), RETRY_FALLBACK_BACKOFF);
}

#[test]
fn parsed_below_fallback_clamps_up() {
    // Any hint smaller than the fallback floor must be lifted, not
    // honored verbatim.
    let msg = "Will retry in 0s.";
    assert!(retry_backoff_for(msg) >= RETRY_FALLBACK_BACKOFF);
}

#[test]
fn fallback_is_at_least_one_log_line_apart() {
    // Sanity: fallback must be large enough to prevent the runaway-loop
    // scenario this fix addresses (sub-millisecond retries flooding logs).
    assert!(
        RETRY_FALLBACK_BACKOFF >= Duration::from_millis(100),
        "fallback backoff too small to prevent log/CPU runaway",
    );
}
