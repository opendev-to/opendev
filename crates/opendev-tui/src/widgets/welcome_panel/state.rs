//! WelcomePanelState — animation state ticked by App.

use super::color::{RainColumn, pseudo_rand};
use crate::widgets::spinner::SPINNER_FRAMES;

/// Persistent animation state for the welcome panel.
#[derive(Debug, Clone)]
pub struct WelcomePanelState {
    pub(super) gradient_offset: u16,
    pub(super) braille_offset: usize,
    pub(super) braille_tick: u8,
    pub(super) breathe_phase: f64,
    pub(super) rain_columns: Vec<RainColumn>,
    pub(super) rain_width: usize,
    pub(super) rain_height: usize,
    pub(super) fade_progress: f32,
    /// Whether the panel is currently fading out.
    pub is_fading: bool,
    /// Set to `true` once the fade-out completes; the panel should no longer be rendered.
    pub fade_complete: bool,
    pub(super) rng_seed: u64,
}

impl WelcomePanelState {
    pub fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        Self {
            gradient_offset: 0,
            braille_offset: 0,
            braille_tick: 0,
            breathe_phase: 0.0,
            rain_columns: Vec::new(),
            rain_width: 0,
            rain_height: 0,
            fade_progress: 1.0,
            is_fading: false,
            fade_complete: false,
            rng_seed: seed,
        }
    }

    /// Advance animations by one tick (~60ms).
    pub fn tick(&mut self, _terminal_width: u16, _terminal_height: u16) {
        if self.is_fading {
            self.fade_progress -= 0.1;
            if self.fade_progress <= 0.0 {
                self.fade_progress = 0.0;
                self.fade_complete = true;
            }
            return;
        }

        // Gradient rotation (slower, more elegant sweep)
        self.gradient_offset = (self.gradient_offset + 3) % 360;

        // Braille offset advances every 2 ticks
        self.braille_tick += 1;
        if self.braille_tick >= 2 {
            self.braille_tick = 0;
            self.braille_offset = (self.braille_offset + 1) % SPINNER_FRAMES.len();
        }

        // Breathing phase: full cycle in ~67 ticks (~4s at 60ms tick)
        self.breathe_phase += std::f64::consts::TAU / 67.0;
        if self.breathe_phase >= std::f64::consts::TAU {
            self.breathe_phase -= std::f64::consts::TAU;
        }

        // Advance rain
        self.step_rain();
    }

    /// Begin the fade-out animation.
    pub fn start_fade(&mut self) {
        self.is_fading = true;
    }

    /// Lazily initialize or resize the rain field.
    pub fn ensure_rain_field(&mut self, w: usize, h: usize) {
        if self.rain_width == w && self.rain_height == h && !self.rain_columns.is_empty() {
            return;
        }
        self.rain_width = w;
        self.rain_height = h;
        self.rain_columns.clear();
        self.rain_columns.reserve(w);
        for _ in 0..w {
            let y = pseudo_rand(&mut self.rng_seed) * h as f32;
            let speed = 0.10 + pseudo_rand(&mut self.rng_seed) * 0.40;
            let trail_len = 4 + (pseudo_rand(&mut self.rng_seed) * 6.0) as u8;
            let char_offset = (pseudo_rand(&mut self.rng_seed) * SPINNER_FRAMES.len() as f32) as u8;
            let hue_offset = pseudo_rand(&mut self.rng_seed) * 30.0 - 15.0; // -15..+15
            self.rain_columns.push(RainColumn {
                y,
                speed,
                trail_len,
                char_offset,
                hue_offset,
            });
        }
    }

    /// Advance rain drop positions, resetting off-screen columns.
    pub(super) fn step_rain(&mut self) {
        let h = self.rain_height as f32;
        if h <= 0.0 {
            return;
        }
        for col in &mut self.rain_columns {
            col.y += col.speed;
            if col.y > h + col.trail_len as f32 {
                col.y = -(pseudo_rand(&mut (col.char_offset as u64 ^ 0xDEAD_BEEF)) * 6.0);
                col.speed = 0.10 + (col.char_offset as f32 % 9.0) * 0.04;
            }
        }
    }
}

impl Default for WelcomePanelState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
