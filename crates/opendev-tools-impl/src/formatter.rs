//! Auto-formatting support for edited files.
//!
//! Detects system-available formatters (rustfmt, prettier, black, gofmt, etc.)
//! and runs the appropriate one after file write/edit operations.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::sync::{LazyLock, Mutex};

use tracing::debug;

/// Information about a formatter.
#[derive(Debug, Clone)]
pub struct FormatterInfo {
    /// Formatter name (e.g., "rustfmt").
    pub name: &'static str,
    /// Command to invoke.
    pub command: &'static str,
    /// File extensions this formatter handles.
    pub extensions: &'static [&'static str],
    /// Whether the formatter is available on the system.
    pub available: bool,
}

/// Formatter definition (static config).
struct FormatterDef {
    name: &'static str,
    command: &'static str,
    /// Arguments template. `{file}` is replaced with the actual file path.
    args: &'static [&'static str],
    extensions: &'static [&'static str],
}

/// Known formatters and their configurations.
const FORMATTERS: &[FormatterDef] = &[
    FormatterDef {
        name: "rustfmt",
        command: "rustfmt",
        args: &["{file}"],
        extensions: &[".rs"],
    },
    FormatterDef {
        name: "black",
        command: "black",
        args: &["--quiet", "--line-length", "100", "{file}"],
        extensions: &[".py"],
    },
    FormatterDef {
        name: "prettier",
        command: "prettier",
        args: &["--write", "{file}"],
        extensions: &[
            ".js", ".jsx", ".ts", ".tsx", ".css", ".scss", ".html", ".json", ".md", ".yaml", ".yml",
        ],
    },
    FormatterDef {
        name: "gofmt",
        command: "gofmt",
        args: &["-w", "{file}"],
        extensions: &[".go"],
    },
    FormatterDef {
        name: "clang-format",
        command: "clang-format",
        args: &["-i", "{file}"],
        extensions: &[".c", ".cpp", ".h", ".hpp", ".cc", ".cxx"],
    },
    FormatterDef {
        name: "shfmt",
        command: "shfmt",
        args: &["-w", "{file}"],
        extensions: &[".sh", ".bash"],
    },
    FormatterDef {
        name: "isort",
        command: "isort",
        args: &["--quiet", "{file}"],
        extensions: &[".py"],
    },
    FormatterDef {
        name: "ruff",
        command: "ruff",
        args: &["format", "{file}"],
        extensions: &[".py", ".pyi"],
    },
    FormatterDef {
        name: "biome",
        command: "biome",
        args: &["check", "--write", "{file}"],
        extensions: &[
            ".js", ".jsx", ".ts", ".tsx", ".css", ".scss", ".json", ".jsonc",
        ],
    },
    FormatterDef {
        name: "mix",
        command: "mix",
        args: &["format", "{file}"],
        extensions: &[".ex", ".exs", ".eex", ".heex", ".leex"],
    },
    FormatterDef {
        name: "zig",
        command: "zig",
        args: &["fmt", "{file}"],
        extensions: &[".zig", ".zon"],
    },
    FormatterDef {
        name: "dart",
        command: "dart",
        args: &["format", "{file}"],
        extensions: &[".dart"],
    },
    FormatterDef {
        name: "ktlint",
        command: "ktlint",
        args: &["-F", "{file}"],
        extensions: &[".kt", ".kts"],
    },
    FormatterDef {
        name: "ocamlformat",
        command: "ocamlformat",
        args: &["-i", "{file}"],
        extensions: &[".ml", ".mli"],
    },
    FormatterDef {
        name: "terraform",
        command: "terraform",
        args: &["fmt", "{file}"],
        extensions: &[".tf", ".tfvars"],
    },
    FormatterDef {
        name: "gleam",
        command: "gleam",
        args: &["format", "{file}"],
        extensions: &[".gleam"],
    },
    FormatterDef {
        name: "nixfmt",
        command: "nixfmt",
        args: &["{file}"],
        extensions: &[".nix"],
    },
    FormatterDef {
        name: "rubocop",
        command: "rubocop",
        args: &["--autocorrect", "{file}"],
        extensions: &[".rb", ".rake", ".gemspec"],
    },
    FormatterDef {
        name: "ormolu",
        command: "ormolu",
        args: &["-i", "{file}"],
        extensions: &[".hs"],
    },
    FormatterDef {
        name: "latexindent",
        command: "latexindent",
        args: &["-w", "-s", "{file}"],
        extensions: &[".tex"],
    },
    FormatterDef {
        name: "dfmt",
        command: "dfmt",
        args: &["-i", "{file}"],
        extensions: &[".d"],
    },
    FormatterDef {
        name: "cljfmt",
        command: "cljfmt",
        args: &["fix", "--quiet", "{file}"],
        extensions: &[".clj", ".cljs", ".cljc", ".edn"],
    },
    FormatterDef {
        name: "swift-format",
        command: "swift-format",
        args: &["--in-place", "{file}"],
        extensions: &[".swift"],
    },
    FormatterDef {
        name: "xmllint",
        command: "xmllint",
        args: &["--format", "--output", "{file}", "{file}"],
        extensions: &[".xml", ".xsl", ".xslt"],
    },
];

/// Global formatter manager state.
struct FormatterState {
    /// Detected formatter availability (name -> available).
    detected: Vec<FormatterInfo>,
    /// Formatters explicitly disabled by the user.
    disabled: HashSet<String>,
    /// Whether detection has run.
    initialized: bool,
}

static STATE: LazyLock<Mutex<FormatterState>> = LazyLock::new(|| {
    Mutex::new(FormatterState {
        detected: Vec::new(),
        disabled: HashSet::new(),
        initialized: false,
    })
});

/// Check if a command is available on the system PATH.
fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Detect which formatters are available on the system.
/// Results are cached after the first call.
pub fn detect_formatters() -> Vec<FormatterInfo> {
    let mut state = STATE.lock().unwrap();
    if state.initialized {
        return state.detected.clone();
    }

    state.detected = FORMATTERS
        .iter()
        .map(|def| FormatterInfo {
            name: def.name,
            command: def.command,
            extensions: def.extensions,
            available: command_exists(def.command),
        })
        .collect();

    let found: Vec<&str> = state
        .detected
        .iter()
        .filter(|f| f.available)
        .map(|f| f.name)
        .collect();
    if !found.is_empty() {
        debug!("Detected {} formatters: {}", found.len(), found.join(", "));
    }

    state.initialized = true;
    state.detected.clone()
}

/// Apply formatter config overrides from `AppConfig.formatter`.
///
/// Disables formatters marked as `disabled: true`, and registers custom
/// formatters with their specified command and extensions.
pub fn apply_config(config: &opendev_models::config::FormatterConfig) {
    if config.is_disabled() {
        // Disable all formatters
        let mut state = STATE.lock().unwrap();
        let names: Vec<String> = state.detected.iter().map(|f| f.name.to_string()).collect();
        for name in names {
            state.disabled.insert(name);
        }
        debug!("All formatters disabled via config");
        return;
    }

    let overrides = config.overrides();
    let mut state = STATE.lock().unwrap();
    for (name, override_cfg) in overrides {
        if override_cfg.disabled {
            state.disabled.insert(name.clone());
            debug!(name = %name, "Formatter disabled via config");
        }
        // Custom formatter commands are handled at format_file time via config lookup
    }
}

/// Format a file using a custom formatter from config (if available).
///
/// Returns `Some(true)` if a custom formatter ran successfully,
/// `Some(false)` if it ran but failed,
/// `None` if no custom formatter matched.
pub fn format_file_with_config(
    file_path: &str,
    working_dir: &Path,
    config: &opendev_models::config::FormatterConfig,
) -> Option<bool> {
    if config.is_disabled() {
        return Some(false);
    }

    let ext = Path::new(file_path)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))?;

    // Check custom formatters from config
    for (name, override_cfg) in config.overrides() {
        if override_cfg.disabled || override_cfg.command.is_empty() {
            continue;
        }
        if !override_cfg.extensions.is_empty() && !override_cfg.extensions.contains(&ext) {
            continue;
        }

        let args: Vec<String> = override_cfg
            .command
            .iter()
            .map(|a| a.replace("$FILE", file_path).replace("{file}", file_path))
            .collect();

        if args.is_empty() {
            continue;
        }

        let mut cmd = Command::new(&args[0]);
        cmd.args(&args[1..]).current_dir(working_dir);
        for (k, v) in &override_cfg.environment {
            cmd.env(k, v);
        }

        match cmd.output() {
            Ok(output) if output.status.success() => {
                debug!("Formatted {} with custom formatter {}", file_path, name);
                return Some(true);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(
                    "Custom formatter {} failed on {}: {}",
                    name,
                    file_path,
                    &stderr[..stderr.len().min(200)]
                );
                return Some(false);
            }
            Err(e) => {
                debug!("Failed to run custom formatter {}: {}", name, e);
                return Some(false);
            }
        }
    }

    None // No custom formatter matched
}

/// Get the best formatter for a given file path.
pub fn get_formatter_for_file(file_path: &str) -> Option<&'static str> {
    let ext = Path::new(file_path)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))?;

    let state = {
        // Ensure detection has run
        drop(detect_formatters());
        STATE.lock().unwrap()
    };

    for info in &state.detected {
        if state.disabled.contains(info.name) {
            continue;
        }
        if info.available && info.extensions.contains(&ext.as_str()) {
            return Some(info.name);
        }
    }
    None
}

/// Run the appropriate formatter on a file.
///
/// Returns `true` if formatting was applied, `false` otherwise.
/// Never panics or returns errors — formatting is best-effort.
pub fn format_file(file_path: &str, working_dir: &Path) -> bool {
    let formatter_name = match get_formatter_for_file(file_path) {
        Some(name) => name,
        None => return false,
    };

    let def = match FORMATTERS.iter().find(|d| d.name == formatter_name) {
        Some(d) => d,
        None => return false,
    };

    let args: Vec<String> = def
        .args
        .iter()
        .map(|a| a.replace("{file}", file_path))
        .collect();

    match Command::new(def.command)
        .args(&args)
        .current_dir(working_dir)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                debug!("Formatted {} with {}", file_path, formatter_name);
                true
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(
                    "Formatter {} failed on {}: {}",
                    formatter_name,
                    file_path,
                    &stderr[..stderr.len().min(200)]
                );
                false
            }
        }
        Err(_) => false,
    }
}

/// Disable a specific formatter by name.
pub fn disable_formatter(name: &str) {
    let mut state = STATE.lock().unwrap();
    state.disabled.insert(name.to_string());
}

/// Enable a previously disabled formatter.
pub fn enable_formatter(name: &str) {
    let mut state = STATE.lock().unwrap();
    state.disabled.remove(name);
}

/// Get status of all formatters.
pub fn get_status() -> Vec<FormatterInfo> {
    detect_formatters()
}

#[cfg(test)]
mod tests {
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
}
