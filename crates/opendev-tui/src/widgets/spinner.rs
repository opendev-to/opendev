//! Animated spinner using braille dot characters.
//!
//! Mirrors the Python `TaskProgressDisplay.SPINNER_FRAMES` and `SpinnerType` system.
//! Provides consistent spinner animation for tool execution, thinking phases,
//! and agent activity indicators.

use super::thinking_verbs::{self, THINKING_VERBS};

/// Braille-dot spinner frames (matches Python `SPINNER_FRAMES`).
pub const SPINNER_FRAMES: &[char] = &[
    '\u{280b}', // ⠋
    '\u{2819}', // ⠙
    '\u{2839}', // ⠹
    '\u{2838}', // ⠸
    '\u{283c}', // ⠼
    '\u{2834}', // ⠴
    '\u{2826}', // ⠦
    '\u{2827}', // ⠧
    '\u{2807}', // ⠇
    '\u{280f}', // ⠏
];

/// Compaction spinner character (matches Claude Code `✻`).
pub const COMPACTION_CHAR: char = '\u{273b}'; // ✻

/// Completed/stopped indicator (matches Python `⏺`).
pub const COMPLETED_CHAR: char = '\u{23fa}'; // ⏺

/// Continuation indicator (matches Python `⎿`).
pub const CONTINUATION_CHAR: char = '\u{23bf}'; // ⎿

/// Success checkmark.
pub const SUCCESS_CHAR: char = '\u{2713}'; // ✓

/// Failure cross.
pub const FAILURE_CHAR: char = '\u{2717}'; // ✗

/// Tree connector characters for nested tool display.
pub const TREE_BRANCH: &str = "\u{251c}\u{2500}"; // ├─
pub const TREE_LAST: &str = "\u{2514}\u{2500}"; // └─
pub const TREE_VERTICAL: &str = "\u{2502}"; // │

/// Spinner state tracker for animation.
#[derive(Debug, Clone)]
pub struct SpinnerState {
    /// Current frame index.
    frame_index: usize,
    /// Monotonic tick counter (incremented each animation frame).
    tick_count: u64,
    /// Current index into the THINKING_VERBS array.
    verb_index: usize,
    /// Tick counter within the current verb's animation cycle.
    verb_tick: u64,
}

impl SpinnerState {
    /// Create a new spinner state starting at frame 0.
    pub fn new() -> Self {
        Self {
            frame_index: 0,
            tick_count: 0,
            verb_index: 0,
            verb_tick: 0,
        }
    }

    /// Advance to the next frame and return the current character.
    pub fn tick(&mut self) -> char {
        let ch = SPINNER_FRAMES[self.frame_index];
        self.frame_index = (self.frame_index + 1) % SPINNER_FRAMES.len();
        self.tick_count += 1;

        // Advance verb animation
        self.verb_tick += 1;
        let current_verb = THINKING_VERBS[self.verb_index];
        if self.verb_tick >= thinking_verbs::cycle_ticks_for(current_verb) {
            self.verb_tick = 0;
            self.verb_index = thinking_verbs::next_verb_index(self.verb_index);
        }

        ch
    }

    /// Get the current character without advancing.
    pub fn current(&self) -> char {
        SPINNER_FRAMES[self.frame_index]
    }

    /// Get the total number of ticks elapsed.
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Get the current thinking verb (full text).
    pub fn current_verb(&self) -> &'static str {
        THINKING_VERBS[self.verb_index]
    }

    /// Get the fade-in intensity for the current verb (0.0 = dim, 1.0 = bright).
    pub fn verb_fade_intensity(&self) -> f32 {
        let verb = THINKING_VERBS[self.verb_index];
        thinking_verbs::compute_fade_intensity(verb, self.verb_tick)
    }

    /// Reset to initial state.
    pub fn reset(&mut self) {
        self.frame_index = 0;
        self.tick_count = 0;
        self.verb_index = 0;
        self.verb_tick = 0;
    }
}

impl Default for SpinnerState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "spinner_tests.rs"]
mod tests;
