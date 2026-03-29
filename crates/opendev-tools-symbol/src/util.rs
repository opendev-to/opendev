//! Shared utilities for symbol tools.

use std::path::Path;

/// Validate that a string is a valid identifier (letter/underscore start, alphanumeric/underscore rest).
pub fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Detect file language category from extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangCategory {
    Python,
    CLike,
    Other,
}

#[allow(dead_code)]
pub fn detect_lang(path: &Path) -> LangCategory {
    match path.extension().and_then(|e| e.to_str()) {
        Some("py" | "pyi" | "pyw") => LangCategory::Python,
        Some(
            "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "java" | "js" | "ts" | "tsx" | "jsx" | "go"
            | "rs" | "cs" | "swift" | "kt" | "scala" | "m" | "mm",
        ) => LangCategory::CLike,
        _ => LangCategory::Other,
    }
}

/// Truncate a string to max chars, appending "..." if truncated.
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

/// Make a path relative to a base, falling back to absolute.
pub fn relative_display(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

#[cfg(test)]
#[path = "util_tests.rs"]
mod tests;
