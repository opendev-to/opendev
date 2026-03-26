//! Animated spinner using braille dot characters.
//!
//! Mirrors the Python `TaskProgressDisplay.SPINNER_FRAMES` and `SpinnerType` system.
//! Provides consistent spinner animation for tool execution, thinking phases,
//! and agent activity indicators.

use super::thinking_verbs::{
    self, THINKING_VERBS,
};

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

    /// Get the current thinking verb text (typewriter-revealed).
    pub fn current_verb(&self) -> &str {
        let verb = THINKING_VERBS[self.verb_index];
        thinking_verbs::compute_verb_text(verb, self.verb_tick)
    }

    /// Whether the current verb is fully revealed (for ellipsis display).
    pub fn is_verb_fully_revealed(&self) -> bool {
        let verb = THINKING_VERBS[self.verb_index];
        thinking_verbs::is_fully_revealed(verb, self.verb_tick)
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
mod tests {
    use super::*;

    #[test]
    fn test_spinner_cycles() {
        let mut spinner = SpinnerState::new();
        let first = spinner.tick();
        assert_eq!(first, '\u{280b}');

        // Cycle through all frames
        for _ in 1..SPINNER_FRAMES.len() {
            spinner.tick();
        }
        // Should be back to first frame
        assert_eq!(spinner.current(), '\u{280b}');
    }

    #[test]
    fn test_spinner_tick_count() {
        let mut spinner = SpinnerState::new();
        assert_eq!(spinner.tick_count(), 0);
        spinner.tick();
        spinner.tick();
        assert_eq!(spinner.tick_count(), 2);
    }

    #[test]
    fn test_spinner_reset() {
        let mut spinner = SpinnerState::new();
        spinner.tick();
        spinner.tick();
        spinner.reset();
        assert_eq!(spinner.tick_count(), 0);
        assert_eq!(spinner.current(), SPINNER_FRAMES[0]);
        assert_eq!(spinner.current_verb(), "T"); // reset to first verb, tick 0
    }

    #[test]
    fn test_verb_advances() {
        let mut spinner = SpinnerState::new();
        let first_verb = THINKING_VERBS[0];
        // Tick past the first verb's full cycle
        let cycle = thinking_verbs::cycle_ticks_for(first_verb);
        for _ in 0..cycle {
            spinner.tick();
        }
        // Should now be on a different verb
        assert_ne!(spinner.current_verb(), &first_verb[..1]);
    }

    #[test]
    fn test_verb_no_immediate_repeat() {
        let mut spinner = SpinnerState::new();
        let first_idx = spinner.verb_index;
        let cycle = thinking_verbs::cycle_ticks_for(THINKING_VERBS[first_idx]);
        for _ in 0..cycle {
            spinner.tick();
        }
        assert_ne!(spinner.verb_index, first_idx);
    }
}
