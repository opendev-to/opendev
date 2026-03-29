//! Centralized color palette and box-drawing constants.
//!
//! Mirrors Python's `style_tokens.py` for consistent styling across the TUI.
//! Supports multiple themes via the [`Theme`] struct.

use ratatui::style::Color;

/// Represents a complete color theme for the TUI.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: &'static str,

    // Core palette
    pub primary: Color,
    pub accent: Color,
    pub subtle: Color,
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub blue_bright: Color,
    pub blue_path: Color,
    pub gold: Color,
    pub border: Color,
    pub border_accent: Color,

    // Semantic colors
    pub grey: Color,
    pub thinking_bg: Color,
    pub orange: Color,
    pub green_light: Color,
    pub green_bright: Color,
    pub blue_task: Color,
    pub blue_light: Color,
    pub orange_caution: Color,
    pub cyan: Color,
    pub dim_grey: Color,

    // Thinking phases
    pub phase_thinking: Color,
    pub phase_critique: Color,
    pub phase_refinement: Color,

    // Markdown heading colors
    pub heading_1: Color,
    pub heading_2: Color,
    pub heading_3: Color,
    pub code_fg: Color,
    pub code_bg: Color,
    pub bullet: Color,
    pub bold_fg: Color,
}

/// Available theme names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    Dark,
    Light,
    Dracula,
}

impl ThemeName {
    /// Parse a theme name from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            "dracula" => Some(Self::Dracula),
            _ => None,
        }
    }

    /// Get the theme struct for this name.
    pub fn theme(self) -> Theme {
        match self {
            Self::Dark => Theme::dark(),
            Self::Light => Theme::light(),
            Self::Dracula => Theme::dracula(),
        }
    }

    /// All available theme names.
    pub fn all() -> &'static [ThemeName] {
        &[Self::Dark, Self::Light, Self::Dracula]
    }

    /// Display name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Dracula => "dracula",
        }
    }
}

impl std::fmt::Display for ThemeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Detected terminal background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalBackground {
    Dark,
    Light,
    Unknown,
}

/// Detect the terminal background color from the `COLORFGBG` environment variable.
///
/// `COLORFGBG` is typically set by terminal emulators in the format "fg;bg" where
/// higher bg values (>= 8) indicate a light background. Falls back to `Unknown`.
pub fn detect_terminal_background() -> TerminalBackground {
    match std::env::var("COLORFGBG") {
        Ok(val) => {
            // Format is typically "fg;bg" (e.g., "15;0" for light-on-dark)
            if let Some(bg_str) = val.rsplit(';').next()
                && let Ok(bg) = bg_str.trim().parse::<u32>()
            {
                // Terminal color indices: 0-6 are dark colors, 7+ are light
                if bg >= 8 {
                    return TerminalBackground::Light;
                } else {
                    return TerminalBackground::Dark;
                }
            }
            TerminalBackground::Unknown
        }
        Err(_) => TerminalBackground::Unknown,
    }
}

/// Select the best theme based on terminal background detection.
/// If background is light, use the light theme. Otherwise default to dark.
pub fn auto_detect_theme() -> ThemeName {
    match detect_terminal_background() {
        TerminalBackground::Light => ThemeName::Light,
        TerminalBackground::Dark | TerminalBackground::Unknown => ThemeName::Dark,
    }
}

impl Theme {
    /// Dark theme — the original default color scheme.
    pub fn dark() -> Self {
        Self {
            name: "dark",
            primary: Color::Rgb(208, 212, 220),
            accent: Color::Rgb(130, 160, 255),
            subtle: Color::Rgb(154, 160, 172),
            success: Color::Rgb(106, 209, 143),
            error: Color::Rgb(255, 92, 87),
            warning: Color::Rgb(255, 179, 71),
            blue_bright: Color::Rgb(74, 158, 255),
            blue_path: Color::Rgb(88, 166, 255),
            gold: Color::Rgb(255, 215, 0),
            border: Color::Rgb(88, 88, 88),
            border_accent: Color::Rgb(147, 147, 255),

            grey: Color::Rgb(122, 126, 134),
            thinking_bg: Color::Rgb(105, 105, 105),
            orange: Color::Rgb(255, 140, 0),
            green_light: Color::Rgb(137, 209, 133),
            green_bright: Color::Rgb(0, 255, 0),
            blue_task: Color::Rgb(37, 150, 190),
            blue_light: Color::Rgb(156, 207, 253),
            orange_caution: Color::Rgb(255, 165, 0),
            cyan: Color::Rgb(0, 191, 255),
            dim_grey: Color::Rgb(107, 114, 128),

            phase_thinking: Color::Rgb(105, 105, 105),
            phase_critique: Color::Rgb(255, 179, 71),
            phase_refinement: Color::Rgb(0, 191, 255),

            heading_1: Color::Rgb(208, 212, 220),
            heading_2: Color::Rgb(208, 212, 220),
            heading_3: Color::Rgb(208, 212, 220),
            code_fg: Color::Rgb(120, 190, 180),
            code_bg: Color::Rgb(30, 30, 30),
            bullet: Color::Rgb(208, 212, 220),
            bold_fg: Color::Rgb(208, 212, 220),
        }
    }

    /// Light theme — optimized for light terminal backgrounds.
    pub fn light() -> Self {
        Self {
            name: "light",
            primary: Color::Rgb(30, 30, 30),
            accent: Color::Rgb(60, 90, 200),
            subtle: Color::Rgb(100, 100, 110),
            success: Color::Rgb(30, 140, 60),
            error: Color::Rgb(200, 40, 40),
            warning: Color::Rgb(180, 120, 0),
            blue_bright: Color::Rgb(30, 100, 200),
            blue_path: Color::Rgb(40, 110, 200),
            gold: Color::Rgb(180, 150, 0),
            border: Color::Rgb(180, 180, 180),
            border_accent: Color::Rgb(100, 100, 200),

            grey: Color::Rgb(120, 120, 130),
            thinking_bg: Color::Rgb(220, 220, 230),
            orange: Color::Rgb(200, 100, 0),
            green_light: Color::Rgb(40, 150, 60),
            green_bright: Color::Rgb(0, 160, 0),
            blue_task: Color::Rgb(20, 110, 160),
            blue_light: Color::Rgb(60, 130, 200),
            orange_caution: Color::Rgb(200, 120, 0),
            cyan: Color::Rgb(0, 140, 200),
            dim_grey: Color::Rgb(140, 140, 150),

            phase_thinking: Color::Rgb(200, 200, 210),
            phase_critique: Color::Rgb(180, 120, 0),
            phase_refinement: Color::Rgb(0, 140, 200),

            heading_1: Color::Rgb(30, 30, 30),
            heading_2: Color::Rgb(30, 30, 30),
            heading_3: Color::Rgb(30, 30, 30),
            code_fg: Color::Rgb(50, 130, 120),
            code_bg: Color::Rgb(240, 240, 240),
            bullet: Color::Rgb(30, 30, 30),
            bold_fg: Color::Rgb(30, 30, 30),
        }
    }

    /// Dracula theme — based on the popular Dracula color scheme.
    pub fn dracula() -> Self {
        Self {
            name: "dracula",
            primary: Color::Rgb(248, 248, 242),       // Foreground
            accent: Color::Rgb(189, 147, 249),        // Purple
            subtle: Color::Rgb(98, 114, 164),         // Comment
            success: Color::Rgb(80, 250, 123),        // Green
            error: Color::Rgb(255, 85, 85),           // Red
            warning: Color::Rgb(255, 184, 108),       // Orange
            blue_bright: Color::Rgb(139, 233, 253),   // Cyan
            blue_path: Color::Rgb(139, 233, 253),     // Cyan
            gold: Color::Rgb(241, 250, 140),          // Yellow
            border: Color::Rgb(68, 71, 90),           // Current Line
            border_accent: Color::Rgb(189, 147, 249), // Purple

            grey: Color::Rgb(98, 114, 164),            // Comment
            thinking_bg: Color::Rgb(68, 71, 90),       // Current Line
            orange: Color::Rgb(255, 184, 108),         // Orange
            green_light: Color::Rgb(80, 250, 123),     // Green
            green_bright: Color::Rgb(80, 250, 123),    // Green
            blue_task: Color::Rgb(139, 233, 253),      // Cyan
            blue_light: Color::Rgb(139, 233, 253),     // Cyan
            orange_caution: Color::Rgb(255, 184, 108), // Orange
            cyan: Color::Rgb(139, 233, 253),           // Cyan
            dim_grey: Color::Rgb(98, 114, 164),        // Comment

            phase_thinking: Color::Rgb(68, 71, 90), // Current Line
            phase_critique: Color::Rgb(255, 184, 108), // Orange
            phase_refinement: Color::Rgb(139, 233, 253), // Cyan

            heading_1: Color::Rgb(248, 248, 242), // Foreground (primary)
            heading_2: Color::Rgb(248, 248, 242), // Foreground (primary)
            heading_3: Color::Rgb(248, 248, 242), // Foreground (primary)
            code_fg: Color::Rgb(150, 170, 220),   // Soft steel blue
            code_bg: Color::Rgb(40, 42, 54),      // Background
            bullet: Color::Rgb(248, 248, 242),    // Foreground (primary)
            bold_fg: Color::Rgb(248, 248, 242),   // Foreground (primary)
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

// ============================================================================
// Legacy constants — kept for backward compatibility.
// These map to the dark theme defaults. New code should use Theme fields.
// ============================================================================

pub const PRIMARY: Color = Color::Rgb(208, 212, 220);
pub const ACCENT: Color = Color::Rgb(130, 160, 255);
pub const SUBTLE: Color = Color::Rgb(154, 160, 172);
pub const SUCCESS: Color = Color::Rgb(106, 209, 143);
pub const ERROR: Color = Color::Rgb(255, 92, 87);
pub const WARNING: Color = Color::Rgb(255, 179, 71);
pub const BLUE_BRIGHT: Color = Color::Rgb(74, 158, 255);
pub const BLUE_PATH: Color = Color::Rgb(88, 166, 255);
pub const GOLD: Color = Color::Rgb(255, 215, 0);
pub const BORDER: Color = Color::Rgb(88, 88, 88);
pub const BORDER_ACCENT: Color = Color::Rgb(147, 147, 255);

// Semantic colors (from Python style_tokens.py)
pub const GREY: Color = Color::Rgb(122, 126, 134);
pub const THINKING_BG: Color = Color::Rgb(105, 105, 105);
pub const ORANGE: Color = Color::Rgb(255, 140, 0);
pub const GREEN_LIGHT: Color = Color::Rgb(137, 209, 133);
pub const GREEN_BRIGHT: Color = Color::Rgb(0, 255, 0);
pub const BLUE_TASK: Color = Color::Rgb(37, 150, 190);
pub const BLUE_LIGHT: Color = Color::Rgb(156, 207, 253);
pub const ORANGE_CAUTION: Color = Color::Rgb(255, 165, 0);
pub const CYAN: Color = Color::Rgb(0, 191, 255);
pub const DIM_GREY: Color = Color::Rgb(107, 114, 128);

// Thinking phases
pub const PHASE_THINKING: Color = Color::Rgb(105, 105, 105);
pub const PHASE_CRITIQUE: Color = Color::Rgb(255, 179, 71);
pub const PHASE_REFINEMENT: Color = Color::Rgb(0, 191, 255);

// Diff background colors (full-line highlight like GitHub)
pub const DIFF_ADD_BG: Color = Color::Rgb(0, 40, 0);
pub const DIFF_DEL_BG: Color = Color::Rgb(40, 0, 0);

// Markdown heading colors
pub const HEADING_1: Color = Color::Rgb(208, 212, 220);
pub const HEADING_2: Color = Color::Rgb(208, 212, 220);
pub const HEADING_3: Color = Color::Rgb(208, 212, 220);
pub const CODE_FG: Color = Color::Rgb(120, 190, 180);
pub const CODE_BG: Color = Color::Rgb(30, 30, 30);
pub const BULLET: Color = Color::Rgb(208, 212, 220);
pub const BOLD_FG: Color = Color::Rgb(208, 212, 220);

// Icons
pub const THINKING_ICON: &str = "\u{27e1}"; // ⟡

/// Build a shimmer/wave line: a bright highlight sweeps across the text.
///
/// `tick` drives the wave position. Each character gets an interpolated color
/// between `base` (dim) and `highlight` (bright) based on distance from the
/// wave center, using a gaussian falloff with a ~4-char wide window.
pub fn shimmer_line(
    text: &str,
    tick: u64,
    base: Color,
    highlight: Color,
) -> Vec<ratatui::text::Span<'static>> {
    use ratatui::style::Style;
    use ratatui::text::Span;

    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if len == 0 {
        return vec![];
    }

    // Wave position: sweep across text, wrapping around with a small gap
    let cycle_len = len + 8; // extra gap before the wave wraps
    let wave_pos = (tick as usize * 2) % cycle_len; // *2 for faster sweep

    let (br, bg, bb) = match base {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => (105.0, 105.0, 105.0),
    };
    let (hr, hg, hb) = match highlight {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => (220.0, 220.0, 230.0),
    };

    let mut spans = Vec::with_capacity(len);
    for (i, ch) in chars.iter().enumerate() {
        let dist = (i as f32 - wave_pos as f32).abs();
        // Gaussian falloff: sigma ~2.0 for a ~4-char wide bright window
        let intensity = (-dist * dist / 8.0).exp();
        let r = (br + (hr - br) * intensity) as u8;
        let g = (bg + (hg - bg) * intensity) as u8;
        let b = (bb + (hb - bb) * intensity) as u8;
        spans.push(Span::styled(
            ch.to_string(),
            Style::default().fg(Color::Rgb(r, g, b)),
        ));
    }
    spans
}

// Box-drawing characters (rounded)
pub const BOX_TL: &str = "\u{256d}";
pub const BOX_TR: &str = "\u{256e}";
pub const BOX_BL: &str = "\u{2570}";
pub const BOX_BR: &str = "\u{256f}";
pub const BOX_H: &str = "\u{2500}";
pub const BOX_V: &str = "\u{2502}";

// Icons
pub const TOOL_HEADER: &str = "\u{23fa}";
pub const INLINE_ARROW: &str = "\u{23bf}";
pub const RESULT_PREFIX: &str = "\u{23bf}  ";

/// Centralized indentation constants for conversation rendering.
/// All conversation line prefixes are defined here — never hardcode indent strings elsewhere.
pub struct Indent;

impl Indent {
    /// 2-space continuation for wrapped lines under a message (matches icon+space width)
    pub const CONT: &str = "  ";
    /// Thinking continuation: vertical line + space (matches "⟡ " width)
    pub const THINKING_CONT: &str = "│ ";
    /// Tool result continuation lines (5 spaces to match "  ⎿  " visual width)
    pub const RESULT_CONT: &str = "     ";

    /// Pre-computed indent strings for common nesting depths (0..=4).
    /// Avoids per-call `CONT.repeat(depth)` allocations in hot rendering paths.
    const DEPTH: [&str; 5] = ["", "  ", "    ", "      ", "        "];

    /// Return a `Cow::Borrowed` indent for common depths, falling back to
    /// `Cow::Owned` with `CONT.repeat(depth)` for deeper nesting.
    pub fn for_depth(depth: usize) -> std::borrow::Cow<'static, str> {
        if depth < Self::DEPTH.len() {
            std::borrow::Cow::Borrowed(Self::DEPTH[depth])
        } else {
            std::borrow::Cow::Owned(Self::CONT.repeat(depth))
        }
    }
}

#[cfg(test)]
#[path = "style_tokens_tests.rs"]
mod tests;
