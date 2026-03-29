use super::*;
use std::collections::HashMap;

#[test]
fn test_detect_formatters_returns_all() {
    let formatters = detect_formatters();
    assert_eq!(formatters.len(), FORMATTERS.len());
    // All names should match
    let names: Vec<&str> = formatters.iter().map(|f| f.name).collect();
    assert!(names.contains(&"rustfmt"));
    assert!(names.contains(&"black"));
    assert!(names.contains(&"prettier"));
    assert!(names.contains(&"gofmt"));
}

#[test]
fn test_get_formatter_for_rust_file() {
    let _ = detect_formatters(); // ensure initialized
    // rustfmt may or may not be available, but if it is, it should match .rs
    if let Some(name) = get_formatter_for_file("src/main.rs") {
        assert_eq!(name, "rustfmt");
    }
}

#[test]
fn test_get_formatter_for_python_file() {
    let _ = detect_formatters();
    if let Some(name) = get_formatter_for_file("script.py") {
        // Should be either "black" or "isort" (whichever is found first)
        assert!(name == "black" || name == "isort");
    }
}

#[test]
fn test_get_formatter_for_unknown_extension() {
    let _ = detect_formatters();
    // .xyz has no formatter
    assert!(get_formatter_for_file("data.xyz").is_none());
}

#[test]
fn test_get_formatter_for_go_file() {
    let _ = detect_formatters();
    if let Some(name) = get_formatter_for_file("main.go") {
        assert_eq!(name, "gofmt");
    }
}

#[test]
fn test_get_formatter_for_js_file() {
    let _ = detect_formatters();
    if let Some(name) = get_formatter_for_file("app.js") {
        assert_eq!(name, "prettier");
    }
}

#[test]
fn test_disable_formatter() {
    let _ = detect_formatters();
    disable_formatter("rustfmt");
    // After disabling, rustfmt should not be returned for .rs files
    // (even if available on the system)
    let result = get_formatter_for_file("test.rs");
    assert!(result.is_none() || result != Some("rustfmt"));
    // Re-enable
    enable_formatter("rustfmt");
}

#[test]
fn test_format_file_nonexistent() {
    // Formatting a nonexistent file should return false, not panic
    let result = format_file("/nonexistent/file.rs", Path::new("/tmp"));
    assert!(!result);
}

#[test]
fn test_formatter_extensions_coverage() {
    // Verify common extensions are covered
    let all_exts: Vec<&str> = FORMATTERS
        .iter()
        .flat_map(|f| f.extensions.iter())
        .copied()
        .collect();
    assert!(all_exts.contains(&".rs"));
    assert!(all_exts.contains(&".py"));
    assert!(all_exts.contains(&".js"));
    assert!(all_exts.contains(&".ts"));
    assert!(all_exts.contains(&".go"));
    assert!(all_exts.contains(&".c"));
    assert!(all_exts.contains(&".sh"));
    assert!(all_exts.contains(&".css"));
    assert!(all_exts.contains(&".html"));
    assert!(all_exts.contains(&".json"));
    // New formatters
    assert!(all_exts.contains(&".zig"));
    assert!(all_exts.contains(&".dart"));
    assert!(all_exts.contains(&".kt"));
    assert!(all_exts.contains(&".ml"));
    assert!(all_exts.contains(&".tf"));
    assert!(all_exts.contains(&".gleam"));
    assert!(all_exts.contains(&".nix"));
    assert!(all_exts.contains(&".rb"));
    assert!(all_exts.contains(&".hs"));
    assert!(all_exts.contains(&".tex"));
    assert!(all_exts.contains(&".d"));
    assert!(all_exts.contains(&".clj"));
    assert!(all_exts.contains(&".swift"));
    assert!(all_exts.contains(&".xml"));
    assert!(all_exts.contains(&".ex"));
}

#[test]
fn test_formatter_count() {
    // We should have 24 built-in formatters
    assert_eq!(FORMATTERS.len(), 24);
}

#[test]
fn test_no_duplicate_formatter_names() {
    let names: Vec<&str> = FORMATTERS.iter().map(|f| f.name).collect();
    let unique: HashSet<&str> = names.iter().copied().collect();
    assert_eq!(names.len(), unique.len(), "Duplicate formatter names found");
}

#[test]
fn test_config_disable_all_formatters() {
    use opendev_models::config::FormatterConfig;
    let config = FormatterConfig::Disabled(false);
    assert!(config.is_disabled());
    assert!(config.overrides().is_empty());
}

#[test]
fn test_config_custom_formatter() {
    use opendev_models::config::{FormatterConfig, FormatterOverride, FormatterOverrides};
    let mut overrides = HashMap::new();
    overrides.insert(
        "my-fmt".to_string(),
        FormatterOverride {
            disabled: false,
            command: vec![
                "my-fmt".to_string(),
                "--write".to_string(),
                "$FILE".to_string(),
            ],
            extensions: vec![".xyz".to_string()],
            environment: HashMap::new(),
        },
    );
    let config = FormatterConfig::Custom(FormatterOverrides { overrides });
    assert!(!config.is_disabled());
    assert!(!config.is_default());
    assert_eq!(config.overrides().len(), 1);
    assert!(config.overrides().contains_key("my-fmt"));
}

#[test]
fn test_config_default_is_empty() {
    use opendev_models::config::FormatterConfig;
    let config = FormatterConfig::default();
    assert!(config.is_default());
    assert!(!config.is_disabled());
}

#[test]
fn test_format_file_with_real_formatter() {
    // If rustfmt is available, test actual formatting
    if !command_exists("rustfmt") {
        return; // Skip if rustfmt not available
    }

    // Ensure rustfmt is enabled (may be disabled by other tests sharing static state)
    enable_formatter("rustfmt");

    let tmp = tempfile::TempDir::new().unwrap();
    let file_path = tmp.path().join("test.rs");
    // Poorly formatted Rust code
    std::fs::write(&file_path, "fn main(){let x=1;println!(\"{}\",x);}\n").unwrap();

    let path_str = file_path.to_str().unwrap();
    let result = format_file(path_str, tmp.path());
    assert!(result, "rustfmt should format successfully");

    // Verify the file was actually formatted
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert!(
        content.contains("fn main()"),
        "formatted code should still have fn main()"
    );
}
