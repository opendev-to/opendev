//! File content injection for `@` mentions with structured XML tags.
//!
//! Mirrors `opendev/repl/file_content_injector.py`.
//!
//! Supports:
//! - Text files: Injected with `<file_content>` tag
//! - Large files: Truncated with head/tail in `<file_truncated>` tag
//! - Directories: Tree listing in `<directory_listing>` tag
//! - PDFs: Extracted text in `<pdf_content>` tag (placeholder)
//! - Images: Multimodal blocks for vision models (base64 encoded)

mod constants;
mod processors;

use constants::*;
use opendev_runtime::gitignore::GitIgnoreParser;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A base64-encoded image block for multimodal API calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBlock {
    /// MIME type, e.g. `"image/png"`.
    pub media_type: String,
    /// Base64-encoded image data.
    pub data: String,
}

/// Result of file content injection.
#[derive(Debug, Clone, Default)]
pub struct InjectionResult {
    /// XML-tagged content for text injection.
    pub text_content: String,
    /// Multimodal image blocks for the API (base64 encoded).
    pub image_blocks: Vec<ImageBlock>,
    /// Error messages for failed references.
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// FileContentInjector
// ---------------------------------------------------------------------------

/// Handles `@` mention file content injection with structured XML tags.
pub struct FileContentInjector {
    /// Working directory for resolving relative paths.
    working_dir: PathBuf,
    /// GitIgnore parser for filtering directory listings.
    pub(super) gitignore: GitIgnoreParser,
}

impl FileContentInjector {
    /// Create a new injector rooted at `working_dir`.
    pub fn new(working_dir: PathBuf) -> Self {
        let working_dir = working_dir
            .canonicalize()
            .unwrap_or_else(|_| working_dir.clone());
        let gitignore = GitIgnoreParser::new(&working_dir);
        Self {
            working_dir,
            gitignore,
        }
    }

    // -- public API ---------------------------------------------------------

    /// Extract `@` references from `query` and inject file contents.
    pub fn inject_content(&self, query: &str) -> InjectionResult {
        let refs = self.extract_refs(query);

        if refs.is_empty() {
            return InjectionResult::default();
        }

        let mut text_parts: Vec<String> = Vec::new();
        let mut image_blocks: Vec<ImageBlock> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for (ref_str, path) in &refs {
            match self.process_ref(ref_str, path) {
                Ok((text, opt_img)) => {
                    text_parts.push(text);
                    if let Some(img) = opt_img {
                        image_blocks.push(img);
                    }
                }
                Err(e) => {
                    text_parts.push(format!(
                        "<file_error path=\"{}\" reason=\"{}\" />",
                        ref_str, e
                    ));
                    errors.push(format!("{}: {}", ref_str, e));
                }
            }
        }

        InjectionResult {
            text_content: text_parts.join("\n\n"),
            image_blocks,
            errors,
        }
    }

    /// Extract file references from a query string.
    ///
    /// Supports:
    /// - Quoted paths: `@"path with spaces/file.py"`
    /// - Unquoted paths: `@main.py`, `@src/utils.py`
    ///
    /// Excludes email addresses like `user@example.com`.
    pub fn extract_refs(&self, query: &str) -> Vec<(String, PathBuf)> {
        let mut refs: Vec<String> = Vec::new();
        let mut seen = HashSet::new();

        // Pattern 1: Quoted paths  @"path with spaces/file.py"
        let quoted_re = Regex::new(r#"@"([^"]+)""#).expect("valid regex");
        for cap in quoted_re.captures_iter(query) {
            let r = cap[1].to_string();
            if seen.insert(r.clone()) {
                refs.push(r);
            }
        }

        // Pattern 2: Unquoted paths
        // Match @ followed by path-like chars, only when @ is at start-of-string,
        // after whitespace, or after non-word character. This avoids emails.
        let unquoted_re = Regex::new(r"(?:^|\s|[^\w])@([a-zA-Z0-9_./\-]+)").expect("valid regex");
        for cap in unquoted_re.captures_iter(query) {
            let r = cap[1].to_string();
            if seen.insert(r.clone()) {
                refs.push(r);
            }
        }

        refs.into_iter()
            .map(|r| {
                let p = self.resolve_path(&r);
                (r, p)
            })
            .collect()
    }

    /// Resolve a reference string to an absolute path.
    pub fn resolve_path(&self, ref_str: &str) -> PathBuf {
        let path = PathBuf::from(ref_str);
        let resolved = if path.is_absolute() {
            path
        } else {
            self.working_dir.join(path)
        };
        // Canonicalize if the path exists; otherwise keep as-is.
        resolved.canonicalize().unwrap_or(resolved)
    }

    /// Check whether a path is a text file suitable for injection.
    pub fn is_text_file(path: &Path) -> bool {
        let ext = ext_lower(path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if SAFE_TEXT_EXTENSIONS.contains(&ext.as_str()) || SAFE_FILENAMES.contains(&name.as_str()) {
            return true;
        }

        if BINARY_EXTENSIONS.contains(&ext.as_str()) {
            return false;
        }

        Self::detect_text_file(path)
    }

    /// Heuristic text-file detection: read 8 KB sample, reject null bytes,
    /// accept valid UTF-8, fallback to printability ratio.
    pub fn detect_text_file(path: &Path) -> bool {
        let sample = match fs::read(path) {
            Ok(data) => {
                if data.len() > 8192 {
                    data[..8192].to_vec()
                } else {
                    data
                }
            }
            Err(_) => return false,
        };

        if sample.is_empty() {
            return true; // empty file counts as text
        }

        // Null bytes are a strong binary indicator.
        if sample.contains(&0u8) {
            return false;
        }

        // Valid UTF-8 ⇒ text
        if std::str::from_utf8(&sample).is_ok() {
            return true;
        }

        // Fallback: check printability ratio on latin-1 interpretation.
        let printable = sample
            .iter()
            .filter(|&&b| {
                b.is_ascii_graphic() || b.is_ascii_whitespace() || (0xA0..=0xFF).contains(&b)
            })
            .count();
        let ratio = printable as f64 / sample.len() as f64;
        ratio > 0.85
    }

    /// Get the syntax-highlighting language for a path.
    pub fn get_language(path: &Path) -> &'static str {
        lang_for_ext(&ext_lower(path))
    }

    /// Format a byte size as a human-readable string.
    pub fn format_size(size: u64) -> String {
        if size < 1024 {
            format!("{}B", size)
        } else if size < 1024 * 1024 {
            format!("{:.1}KB", size as f64 / 1024.0)
        } else {
            format!("{:.1}MB", size as f64 / (1024.0 * 1024.0))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
