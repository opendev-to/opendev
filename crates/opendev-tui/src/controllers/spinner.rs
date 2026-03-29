//! Spinner controller for displaying animated loading indicators.
//!
//! Uses braille animation frames for a smooth terminal spinner.

/// Braille animation frames.
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Controller for a terminal spinner animation.
pub struct SpinnerController {
    current_frame: usize,
    message: String,
    active: bool,
}

impl SpinnerController {
    /// Create a new inactive spinner.
    pub fn new() -> Self {
        Self {
            current_frame: 0,
            message: String::new(),
            active: false,
        }
    }

    /// Whether the spinner is currently active.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The spinner message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Start the spinner with the given message.
    pub fn start(&mut self, message: String) {
        self.message = message;
        self.current_frame = 0;
        self.active = true;
    }

    /// Stop the spinner.
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Advance to the next frame and return the current frame character.
    ///
    /// Returns the braille character for the current frame.
    pub fn tick(&mut self) -> &'static str {
        let frame = SPINNER_FRAMES[self.current_frame];
        self.current_frame = (self.current_frame + 1) % SPINNER_FRAMES.len();
        frame
    }

    /// The available animation frames.
    pub fn frames() -> &'static [&'static str] {
        SPINNER_FRAMES
    }

    /// The current frame index.
    pub fn current_frame(&self) -> usize {
        self.current_frame
    }
}

impl Default for SpinnerController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "spinner_tests.rs"]
mod tests;
