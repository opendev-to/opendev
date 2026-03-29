//! Interrupt manager for signaling cancellation from the TUI.
//!
//! Provides a thread-safe atomic boolean for interrupt signaling between
//! the UI thread and agent/tool execution threads.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Thread-safe interrupt manager using an atomic boolean.
#[derive(Clone)]
pub struct InterruptManager {
    interrupted: Arc<AtomicBool>,
}

impl InterruptManager {
    /// Create a new interrupt manager (not interrupted).
    pub fn new() -> Self {
        Self {
            interrupted: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signal an interrupt.
    pub fn interrupt(&self) {
        self.interrupted.store(true, Ordering::Release);
    }

    /// Clear the interrupt signal.
    pub fn clear(&self) {
        self.interrupted.store(false, Ordering::Release);
    }

    /// Check whether an interrupt has been signaled.
    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::Acquire)
    }
}

impl Default for InterruptManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "interrupt_tests.rs"]
mod tests;
