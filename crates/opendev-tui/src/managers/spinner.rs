//! Spinner service for managing loading indicator state.
//!
//! Tracks whether a spinner is active, its message, and elapsed time
//! since activation.

use std::time::{Duration, Instant};

/// Service for managing spinner animation state and timing.
pub struct SpinnerService {
    active: bool,
    message: String,
    start_time: Option<Instant>,
}

impl SpinnerService {
    /// Create a new inactive spinner service.
    pub fn new() -> Self {
        Self {
            active: false,
            message: String::new(),
            start_time: None,
        }
    }

    /// Whether the spinner is currently active.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The current spinner message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Start the spinner with the given message.
    pub fn start(&mut self, message: String) {
        self.message = message;
        self.start_time = Some(Instant::now());
        self.active = true;
    }

    /// Stop the spinner.
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Get the elapsed duration since the spinner was started.
    ///
    /// Returns `Duration::ZERO` if the spinner was never started.
    pub fn elapsed(&self) -> Duration {
        self.start_time
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO)
    }
}

impl Default for SpinnerService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_inactive() {
        let svc = SpinnerService::new();
        assert!(!svc.active());
        assert!(svc.message().is_empty());
        assert_eq!(svc.elapsed(), Duration::ZERO);
    }

    #[test]
    fn test_start_stop() {
        let mut svc = SpinnerService::new();
        svc.start("Loading models...".into());
        assert!(svc.active());
        assert_eq!(svc.message(), "Loading models...");
        assert!(svc.elapsed() >= Duration::ZERO);

        svc.stop();
        assert!(!svc.active());
    }

    #[test]
    fn test_elapsed_increases() {
        let mut svc = SpinnerService::new();
        svc.start("Working".into());
        std::thread::sleep(Duration::from_millis(1));
        let e = svc.elapsed();
        assert!(e >= Duration::from_millis(1));
    }
}
