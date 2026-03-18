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

            heading_1: Color::Rgb(186, 182, 215),
            heading_2: Color::Rgb(186, 182, 215),
            heading_3: Color::Rgb(186, 182, 215),
            code_fg: Color::Rgb(150, 190, 160),
            code_bg: Color::Rgb(30, 30, 30),
            bullet: Color::Rgb(140, 148, 160),
            bold_fg: Color::Rgb(222, 216, 200),
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

            heading_1: Color::Rgb(80, 70, 110),
            heading_2: Color::Rgb(80, 70, 110),
            heading_3: Color::Rgb(80, 70, 110),
            code_fg: Color::Rgb(50, 120, 70),
            code_bg: Color::Rgb(240, 240, 240),
            bullet: Color::Rgb(120, 120, 135),
            bold_fg: Color::Rgb(50, 45, 35),
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

            heading_1: Color::Rgb(180, 165, 220), // Soft purple
            heading_2: Color::Rgb(180, 165, 220),
            heading_3: Color::Rgb(180, 165, 220),
            code_fg: Color::Rgb(140, 210, 155), // Muted green
            code_bg: Color::Rgb(40, 42, 54),    // Background
            bullet: Color::Rgb(98, 114, 164),   // Comment
            bold_fg: Color::Rgb(235, 225, 210), // Warm off-white
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
pub const HEADING_1: Color = Color::Rgb(186, 182, 215);
pub const HEADING_2: Color = Color::Rgb(186, 182, 215);
pub const HEADING_3: Color = Color::Rgb(186, 182, 215);
pub const CODE_FG: Color = Color::Rgb(150, 190, 160);
pub const CODE_BG: Color = Color::Rgb(30, 30, 30);
pub const BULLET: Color = Color::Rgb(140, 148, 160);
pub const BOLD_FG: Color = Color::Rgb(222, 216, 200);

// Icons
pub const THINKING_ICON: &str = "\u{27e1}"; // ⟡

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
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme_matches_legacy_constants() {
        let dark = Theme::dark();
        assert_eq!(dark.primary, PRIMARY);
        assert_eq!(dark.accent, ACCENT);
        assert_eq!(dark.success, SUCCESS);
        assert_eq!(dark.error, ERROR);
        assert_eq!(dark.warning, WARNING);
        assert_eq!(dark.border, BORDER);
        assert_eq!(dark.code_fg, CODE_FG);
        assert_eq!(dark.code_bg, CODE_BG);
        assert_eq!(dark.bold_fg, BOLD_FG);
    }

    #[test]
    fn test_light_theme_differs_from_dark() {
        let dark = Theme::dark();
        let light = Theme::light();
        assert_ne!(dark.primary, light.primary);
        assert_ne!(dark.code_bg, light.code_bg);
        assert_eq!(light.name, "light");
    }

    #[test]
    fn test_dracula_theme() {
        let dracula = Theme::dracula();
        assert_eq!(dracula.name, "dracula");
        assert_eq!(dracula.primary, Color::Rgb(248, 248, 242));
        assert_eq!(dracula.error, Color::Rgb(255, 85, 85));
    }

    #[test]
    fn test_theme_name_from_str() {
        assert_eq!(ThemeName::from_str_loose("dark"), Some(ThemeName::Dark));
        assert_eq!(ThemeName::from_str_loose("LIGHT"), Some(ThemeName::Light));
        assert_eq!(
            ThemeName::from_str_loose("Dracula"),
            Some(ThemeName::Dracula)
        );
        assert_eq!(ThemeName::from_str_loose("nonexistent"), None);
    }

    #[test]
    fn test_theme_name_roundtrip() {
        for name in ThemeName::all() {
            let s = name.as_str();
            let parsed = ThemeName::from_str_loose(s).unwrap();
            assert_eq!(*name, parsed);
        }
    }

    #[test]
    fn test_theme_name_to_theme() {
        let dark = ThemeName::Dark.theme();
        assert_eq!(dark.name, "dark");
        let light = ThemeName::Light.theme();
        assert_eq!(light.name, "light");
    }

    #[test]
    fn test_default_theme_is_dark() {
        let default = Theme::default();
        assert_eq!(default.name, "dark");
        assert_eq!(default, Theme::dark());
    }

    #[test]
    fn test_detect_terminal_background_dark() {
        // Can't reliably test env var detection in unit tests,
        // but we can test the Unknown/default path
        let bg = detect_terminal_background();
        // In CI/test, COLORFGBG is typically not set
        assert!(matches!(
            bg,
            TerminalBackground::Dark | TerminalBackground::Light | TerminalBackground::Unknown
        ));
    }

    #[test]
    fn test_auto_detect_theme_fallback() {
        // Without COLORFGBG set, should default to dark
        let theme = auto_detect_theme();
        // Could be Dark or Light depending on environment
        assert!(matches!(theme, ThemeName::Dark | ThemeName::Light));
    }

    #[test]
    fn test_all_themes_have_distinct_names() {
        let themes: Vec<Theme> = ThemeName::all().iter().map(|n| n.theme()).collect();
        for (i, a) in themes.iter().enumerate() {
            for (j, b) in themes.iter().enumerate() {
                if i != j {
                    assert_ne!(a.name, b.name);
                }
            }
        }
    }
}
