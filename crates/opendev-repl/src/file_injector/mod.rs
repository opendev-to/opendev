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
mod tests {
    use super::*;
    use base64::Engine as _;
    use std::fs;
    use tempfile::TempDir;

    fn tmp_injector() -> (TempDir, FileContentInjector) {
        let dir = TempDir::new().unwrap();
        let inj = FileContentInjector::new(dir.path().to_path_buf());
        (dir, inj)
    }

    // -- extract_refs -------------------------------------------------------

    #[test]
    fn test_extract_refs_quoted() {
        let (_dir, inj) = tmp_injector();
        let refs = inj.extract_refs(r#"look at @"my file.py""#);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "my file.py");
    }

    #[test]
    fn test_extract_refs_unquoted() {
        let (_dir, inj) = tmp_injector();
        let refs = inj.extract_refs("explain @main.py and @src/utils.rs");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].0, "main.py");
        assert_eq!(refs[1].0, "src/utils.rs");
    }

    #[test]
    fn test_extract_refs_excludes_emails() {
        let (_dir, inj) = tmp_injector();
        let refs = inj.extract_refs("send to user@example.com please");
        assert!(
            refs.is_empty(),
            "emails should not be extracted: {:?}",
            refs
        );
    }

    #[test]
    fn test_extract_refs_dedup() {
        let (_dir, inj) = tmp_injector();
        let refs = inj.extract_refs("@foo.py and @foo.py again");
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn test_extract_refs_mixed() {
        let (_dir, inj) = tmp_injector();
        let refs = inj.extract_refs(r#"@plain.rs and @"quoted path.txt""#);
        assert_eq!(refs.len(), 2);
    }

    // -- is_text_file -------------------------------------------------------

    #[test]
    fn test_is_text_file_known_extensions() {
        let dir = TempDir::new().unwrap();
        for ext in &[".py", ".rs", ".js", ".md", ".json", ".toml", ".yaml"] {
            let p = dir.path().join(format!("test{}", ext));
            fs::write(&p, "content").unwrap();
            assert!(
                FileContentInjector::is_text_file(&p),
                "{} should be text",
                ext
            );
        }
    }

    #[test]
    fn test_is_text_file_known_filenames() {
        let dir = TempDir::new().unwrap();
        for name in &["Dockerfile", "Makefile", "README", "LICENSE"] {
            let p = dir.path().join(name);
            fs::write(&p, "content").unwrap();
            assert!(
                FileContentInjector::is_text_file(&p),
                "{} should be text",
                name
            );
        }
    }

    #[test]
    fn test_is_text_file_binary_extension() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("thing.exe");
        fs::write(&p, "MZ\x00\x00").unwrap();
        assert!(!FileContentInjector::is_text_file(&p));
    }

    #[test]
    fn test_is_text_file_unknown_ext_text() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("data.cfg");
        fs::write(&p, "key = value\nfoo = bar\n").unwrap();
        assert!(FileContentInjector::is_text_file(&p));
    }

    // -- detect_text_file ---------------------------------------------------

    #[test]
    fn test_detect_text_file_with_nulls() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("binary.dat");
        fs::write(&p, b"hello\x00world").unwrap();
        assert!(!FileContentInjector::detect_text_file(&p));
    }

    #[test]
    fn test_detect_text_file_empty() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("empty");
        fs::write(&p, b"").unwrap();
        assert!(FileContentInjector::detect_text_file(&p));
    }

    // -- get_language -------------------------------------------------------

    #[test]
    fn test_get_language_known() {
        assert_eq!(
            FileContentInjector::get_language(Path::new("foo.py")),
            "python"
        );
        assert_eq!(
            FileContentInjector::get_language(Path::new("bar.rs")),
            "rust"
        );
        assert_eq!(
            FileContentInjector::get_language(Path::new("baz.ts")),
            "typescript"
        );
    }

    #[test]
    fn test_get_language_unknown() {
        assert_eq!(FileContentInjector::get_language(Path::new("data.xyz")), "");
    }

    // -- format_size --------------------------------------------------------

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(FileContentInjector::format_size(0), "0B");
        assert_eq!(FileContentInjector::format_size(512), "512B");
        assert_eq!(FileContentInjector::format_size(1023), "1023B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(FileContentInjector::format_size(1024), "1.0KB");
        assert_eq!(FileContentInjector::format_size(2560), "2.5KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(FileContentInjector::format_size(1048576), "1.0MB");
        assert_eq!(FileContentInjector::format_size(5 * 1024 * 1024), "5.0MB");
    }

    // -- process_text_file --------------------------------------------------

    #[test]
    fn test_process_text_file_output() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("hello.py");
        fs::write(&p, "print('hello')").unwrap();
        let result = FileContentInjector::process_text_file(&p, "hello.py").unwrap();
        assert!(result.contains("<file_content"));
        assert!(result.contains("path=\"hello.py\""));
        assert!(result.contains("language=\"python\""));
        assert!(result.contains("print('hello')"));
        assert!(result.contains("</file_content>"));
    }

    // -- process_large_file -------------------------------------------------

    #[test]
    fn test_process_large_file_truncation() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("big.py");

        // Create a file with 2000 lines
        let lines_vec: Vec<String> = (0..2000).map(|i| format!("line {}", i)).collect();
        let content = lines_vec.join("\n");
        fs::write(&p, &content).unwrap();

        let lines: Vec<&str> = content.lines().collect();
        let result = FileContentInjector::process_large_file(&p, "big.py", &content, &lines);
        assert!(result.contains("<file_truncated"));
        assert!(result.contains("total_lines=\"2000\""));
        assert!(result.contains("=== HEAD"));
        assert!(result.contains("=== TRUNCATED"));
        assert!(result.contains("=== TAIL"));
        assert!(result.contains("</file_truncated>"));
    }

    // -- process_directory --------------------------------------------------

    #[test]
    fn test_process_directory_output() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("mydir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("a.txt"), "aaa").unwrap();
        fs::write(sub.join("b.txt"), "bbb").unwrap();

        let inj = FileContentInjector::new(dir.path().to_path_buf());
        let result = inj.process_directory(&sub, "mydir");
        assert!(result.contains("<directory_listing"));
        assert!(result.contains("path=\"mydir\""));
        assert!(result.contains("a.txt"));
        assert!(result.contains("b.txt"));
        assert!(result.contains("</directory_listing>"));
    }

    #[test]
    fn test_process_directory_ignores_git() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("config"), "x").unwrap();
        fs::write(root.join("main.rs"), "fn main() {}").unwrap();

        let inj = FileContentInjector::new(dir.path().to_path_buf());
        let result = inj.process_directory(&root, "proj");
        assert!(!result.contains(".git"));
        assert!(result.contains("main.rs"));
    }

    // -- process_image ------------------------------------------------------

    #[test]
    fn test_process_image_base64() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("logo.png");
        // Minimal 1x1 red PNG
        let png_bytes: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG header
            0x00, 0x00, 0x00, 0x01, // chunk length (fake minimal data)
        ];
        fs::write(&p, png_bytes).unwrap();

        let (tag, block) = FileContentInjector::process_image(&p, "logo.png");
        assert!(tag.contains("<image"));
        assert!(tag.contains("type=\"image/png\""));
        assert!(tag.contains("[Image attached as multimodal content]"));

        let block = block.expect("should produce an ImageBlock");
        assert_eq!(block.media_type, "image/png");
        // Verify the base64 decodes back to original bytes.
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&block.data)
            .unwrap();
        assert_eq!(decoded, png_bytes);
    }

    #[test]
    fn test_process_image_jpeg_mime() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("photo.jpg");
        fs::write(&p, b"fake jpeg data").unwrap();
        let (_tag, block) = FileContentInjector::process_image(&p, "photo.jpg");
        assert_eq!(block.unwrap().media_type, "image/jpeg");
    }

    // -- inject_content (end-to-end) ----------------------------------------

    #[test]
    fn test_inject_content_no_refs() {
        let (_dir, inj) = tmp_injector();
        let result = inj.inject_content("just a plain query");
        assert!(result.text_content.is_empty());
        assert!(result.image_blocks.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_inject_content_file_not_found() {
        let (_dir, inj) = tmp_injector();
        let result = inj.inject_content("look at @nonexistent.py");
        assert!(result.text_content.contains("file_error"));
        assert!(result.text_content.contains("File not found"));
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_inject_content_text_file() {
        let (dir, inj) = tmp_injector();
        let p = dir.path().join("hello.rs");
        fs::write(&p, "fn main() {}").unwrap();

        let result = inj.inject_content("explain @hello.rs");
        assert!(result.text_content.contains("<file_content"));
        assert!(result.text_content.contains("fn main() {}"));
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_inject_content_directory() {
        let (dir, inj) = tmp_injector();
        let sub = dir.path().join("src");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("lib.rs"), "// lib").unwrap();

        let result = inj.inject_content("show me @src");
        assert!(result.text_content.contains("<directory_listing"));
        assert!(result.text_content.contains("lib.rs"));
    }

    #[test]
    fn test_inject_content_image() {
        let (dir, inj) = tmp_injector();
        let p = dir.path().join("pic.png");
        fs::write(&p, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

        let result = inj.inject_content("analyze @pic.png");
        assert!(result.text_content.contains("<image"));
        assert_eq!(result.image_blocks.len(), 1);
    }

    #[test]
    fn test_inject_content_unsupported() {
        let (dir, inj) = tmp_injector();
        let p = dir.path().join("data.exe");
        fs::write(&p, b"\x00\x00\x00\x00").unwrap();

        let result = inj.inject_content("look at @data.exe");
        // .exe is a known binary extension AND an image ext isn't matched,
        // so it goes through process_ref which calls is_text_file => false => Unsupported
        assert!(result.text_content.contains("file_error"));
    }

    // -- resolve_path -------------------------------------------------------

    #[test]
    fn test_resolve_path_relative() {
        let (dir, inj) = tmp_injector();
        let p = dir.path().join("test.py");
        fs::write(&p, "x").unwrap();
        let resolved = inj.resolve_path("test.py");
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with("test.py"));
    }

    #[test]
    fn test_resolve_path_absolute() {
        let (_dir, inj) = tmp_injector();
        let abs_path = std::env::temp_dir().join("some_file.py");
        let abs_str = abs_path.to_str().unwrap();
        let resolved = inj.resolve_path(abs_str);
        assert_eq!(resolved, abs_path);
    }
}
