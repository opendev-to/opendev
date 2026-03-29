//! Color utilities: HSL→RGB conversion, pseudo-random generator, and rain column type.

use ratatui::style::Color;

// ---------------------------------------------------------------------------
// HSL → RGB helper
// ---------------------------------------------------------------------------

/// Convert HSL to ratatui `Color::Rgb`. Hue in 0..360, saturation/lightness in 0.0..1.0.
pub(super) fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> Color {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let h = hue / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = lightness - c / 2.0;
    Color::Rgb(
        ((r1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).clamp(0.0, 255.0) as u8,
    )
}

// ---------------------------------------------------------------------------
// Inline pseudo-random (LCG — no `rand` dependency)
// ---------------------------------------------------------------------------

pub(super) fn pseudo_rand(seed: &mut u64) -> f32 {
    *seed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    (*seed >> 33) as f32 / (1u64 << 31) as f32
}

// ---------------------------------------------------------------------------
// Rain column
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(super) struct RainColumn {
    pub y: f32,
    pub speed: f32,
    pub trail_len: u8,
    pub char_offset: u8,
    pub hue_offset: f32, // per-column hue variation for depth
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "color_tests.rs"]
mod tests;
