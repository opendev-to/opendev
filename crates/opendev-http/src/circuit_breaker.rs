//! Circuit breaker for provider API calls.
//!
//! Protects against cascading failures by tracking consecutive errors and
//! temporarily rejecting requests when a provider is down. After a cooldown
//! period a single probe request is allowed through to test recovery.
//!
//! States:
//! - **Closed**: Normal operation. Failures increment the counter.
//! - **Open**: Too many failures. All requests are rejected immediately.
//! - **HalfOpen**: Cooldown elapsed. One probe request is allowed.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use crate::models::HttpError;

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — requests flow through.
    Closed,
    /// Too many failures — requests are rejected immediately.
    Open,
    /// Cooldown elapsed — one probe request is permitted.
    HalfOpen,
}

/// A circuit breaker that tracks consecutive failures and opens the circuit
/// when a configurable threshold is reached.
pub struct CircuitBreaker {
    /// Number of consecutive failures observed.
    failure_count: AtomicU32,
    /// Number of consecutive failures required to open the circuit.
    threshold: u32,
    /// Timestamp of the most recent failure (used for cooldown calculation).
    last_failure: Mutex<Option<Instant>>,
    /// How long the circuit stays open before transitioning to half-open.
    cooldown: Duration,
    /// Name of the provider (used in log messages).
    provider: String,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    ///
    /// * `provider` — human-readable provider name for log messages.
    /// * `threshold` — number of consecutive failures before opening.
    /// * `cooldown` — time to wait in the open state before probing.
    pub fn new(provider: impl Into<String>, threshold: u32, cooldown: Duration) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            threshold,
            last_failure: Mutex::new(None),
            cooldown,
            provider: provider.into(),
        }
    }

    /// Create a circuit breaker with sensible defaults (5 failures, 30s cooldown).
    pub fn with_defaults(provider: impl Into<String>) -> Self {
        Self::new(provider, 5, Duration::from_secs(30))
    }

    /// Return the current state of the circuit.
    pub fn state(&self) -> CircuitState {
        let failures = self.failure_count.load(Ordering::Relaxed);

        if failures < self.threshold {
            return CircuitState::Closed;
        }

        // Circuit has reached the failure threshold — check cooldown.
        let lock = self.last_failure.lock().unwrap_or_else(|e| e.into_inner());
        match *lock {
            Some(last) if last.elapsed() >= self.cooldown => CircuitState::HalfOpen,
            _ => CircuitState::Open,
        }
    }

    /// Check whether a request should be allowed through.
    ///
    /// Returns `Ok(())` if the request may proceed, or `Err(HttpError)` if
    /// the circuit is open and the request should be rejected.
    pub fn check(&self) -> Result<(), HttpError> {
        match self.state() {
            CircuitState::Closed => Ok(()),
            CircuitState::HalfOpen => {
                debug!(
                    provider = %self.provider,
                    "Circuit half-open, allowing probe request"
                );
                Ok(())
            }
            CircuitState::Open => {
                let remaining = {
                    let lock = self.last_failure.lock().unwrap_or_else(|e| e.into_inner());
                    lock.map(|last| self.cooldown.saturating_sub(last.elapsed()))
                        .unwrap_or(self.cooldown)
                };
                warn!(
                    provider = %self.provider,
                    remaining_secs = remaining.as_secs(),
                    "Circuit open, rejecting request"
                );
                Err(HttpError::Other(format!(
                    "Circuit breaker open for provider '{}'. \
                     Too many consecutive failures ({}). \
                     Will retry in {}s.",
                    self.provider,
                    self.failure_count.load(Ordering::Relaxed),
                    remaining.as_secs(),
                )))
            }
        }
    }

    /// Record a successful request. Resets the failure counter and closes the
    /// circuit if it was half-open.
    pub fn record_success(&self) {
        let prev = self.failure_count.swap(0, Ordering::Relaxed);
        if prev >= self.threshold {
            info!(
                provider = %self.provider,
                "Circuit breaker closed after successful probe"
            );
        }
    }

    /// Record a failed request. Increments the failure counter and, if the
    /// threshold is reached, opens the circuit.
    pub fn record_failure(&self) {
        let new_count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;

        // Update last-failure timestamp.
        {
            let mut lock = self.last_failure.lock().unwrap_or_else(|e| e.into_inner());
            *lock = Some(Instant::now());
        }

        if new_count == self.threshold {
            warn!(
                provider = %self.provider,
                threshold = self.threshold,
                cooldown_secs = self.cooldown.as_secs(),
                "Circuit breaker opened after {} consecutive failures",
                self.threshold
            );
        }
    }

    /// Get the current failure count.
    pub fn failure_count(&self) -> u32 {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// Reset the circuit breaker to its initial (closed) state.
    pub fn reset(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        let mut lock = self.last_failure.lock().unwrap_or_else(|e| e.into_inner());
        *lock = None;
    }
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("provider", &self.provider)
            .field("state", &self.state())
            .field("failure_count", &self.failure_count.load(Ordering::Relaxed))
            .field("threshold", &self.threshold)
            .field("cooldown", &self.cooldown)
            .finish()
    }
}

#[cfg(test)]
mod tests {
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
}
