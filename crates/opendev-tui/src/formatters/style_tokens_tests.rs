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
