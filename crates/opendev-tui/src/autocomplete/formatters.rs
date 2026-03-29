//! Format completion items for popup display.
//!
//! Mirrors the Python `CompletionFormatter` and `get_file_icon` — produces
//! display strings with type indicators, icons, and path shortening.

use std::path::Path;

use super::file_finder::format_file_size;
use super::{CompletionItem, CompletionKind};

// ── File type icon mapping ─────────────────────────────────────────

/// Return a short type tag and a color hint for a file path based on its
/// extension.
///
/// The color string maps to ratatui `Color` names (used by the caller when
/// rendering styled spans).
pub fn file_type_indicator(path: &str) -> (&'static str, &'static str) {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    match ext.to_lowercase().as_str() {
        "py" => ("py", "Cyan"),
        "js" => ("js", "Yellow"),
        "jsx" => ("jsx", "Yellow"),
        "ts" => ("ts", "Blue"),
        "tsx" => ("tsx", "Blue"),
        "rs" => ("rs", "Red"),
        "go" => ("go", "Cyan"),
        "java" => ("java", "Red"),
        "c" | "h" => ("c", "Blue"),
        "cpp" | "cc" | "hpp" => ("cpp", "Blue"),
        "cs" => ("cs", "Green"),
        "rb" => ("rb", "Red"),
        "php" => ("php", "Magenta"),
        "swift" => ("swift", "Yellow"),
        "kt" | "kts" => ("kt", "Magenta"),
        "html" | "htm" => ("html", "Yellow"),
        "css" => ("css", "Blue"),
        "scss" | "sass" => ("scss", "Magenta"),
        "json" => ("json", "Yellow"),
        "xml" => ("xml", "Green"),
        "yaml" | "yml" => ("yaml", "Magenta"),
        "toml" => ("toml", "Magenta"),
        "md" | "markdown" => ("md", "Blue"),
        "txt" => ("txt", "Gray"),
        "sh" | "bash" | "zsh" => ("sh", "Green"),
        "sql" => ("sql", "Yellow"),
        "csv" => ("csv", "Green"),
        "env" => ("env", "Yellow"),
        "lock" => ("lock", "Gray"),
        _ => {
            // Special file names
            match name {
                "Makefile" => ("make", "Red"),
                "Dockerfile" => ("dock", "Blue"),
                _ => {
                    if ext.is_empty() {
                        ("file", "Gray")
                    } else {
                        // Use first 4 chars of extension
                        // We can't return a dynamic &str, so fall back to "file"
                        ("file", "Gray")
                    }
                }
            }
        }
    }
}

/// Shorten a path for display in the popup.
///
/// If the path is longer than `max_len`, the leading components are replaced
/// with `...`.
pub fn shorten_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    // Find a split point so that ".../<rest>" fits in max_len
    let parts: Vec<&str> = path.split('/').collect();
    let mut result = String::new();
    for (i, part) in parts.iter().enumerate().rev() {
        let candidate = if result.is_empty() {
            part.to_string()
        } else {
            format!("{}/{}", part, result)
        };
        if candidate.len() + 4 > max_len && i > 0 {
            // 4 = ".../"
            return format!(".../{}", result);
        }
        result = candidate;
    }
    result
}

// ── CompletionFormatter ────────────────────────────────────────────

/// Formats completion items into display strings.
pub struct CompletionFormatter;

impl CompletionFormatter {
    /// Format a completion item into `(left_label, right_meta)`.
    pub fn format(item: &CompletionItem) -> (String, String) {
        match item.kind {
            CompletionKind::Command => {
                let label = format!("{:<18}", item.label);
                (label, item.description.clone())
            }
            CompletionKind::File => {
                let (type_tag, _color) = file_type_indicator(&item.label);
                let shortened = shorten_path(&item.label, 46);
                let label = format!("{} {:<46}", type_tag, shortened);
                let meta = if item.description.is_empty() {
                    String::new()
                } else {
                    format!("{:>10}", item.description)
                };
                (label, meta)
            }
            CompletionKind::Symbol => {
                let label = format!("{:<30}", item.label);
                (label, item.description.clone())
            }
        }
    }

    /// Get a human-readable file size string for a path, or empty if
    /// the file cannot be stat'd.
    pub fn file_size_string(path: &Path) -> String {
        match std::fs::metadata(path) {
            Ok(meta) => format_file_size(meta.len()),
            Err(_) => String::new(),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "formatters_tests.rs"]
mod tests;
