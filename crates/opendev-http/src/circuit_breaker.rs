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

use serde::{Deserialize, Serialize};
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

/// Configuration for a circuit breaker.
///
/// This struct is serializable so it can be loaded from config files or
/// passed as part of provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit.
    pub failure_threshold: u32,
    /// Seconds the circuit stays open before transitioning to half-open.
    pub reset_timeout_secs: u64,
    /// Seconds between probe attempts in the half-open state.
    /// Defaults to the same value as `reset_timeout_secs` if not set.
    pub probe_interval_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout_secs: 30,
            probe_interval_secs: 30,
        }
    }
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

    /// Create a circuit breaker from a [`CircuitBreakerConfig`].
    pub fn from_config(provider: impl Into<String>, config: &CircuitBreakerConfig) -> Self {
        Self::new(
            provider,
            config.failure_threshold,
            Duration::from_secs(config.reset_timeout_secs),
        )
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
#[path = "circuit_breaker_tests.rs"]
mod tests;
