//! Read file tool — reads file contents with optional line ranges and binary detection.

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::path_utils::{is_sensitive_file, resolve_file_path, validate_path_access};

/// Tool for reading file contents.
#[derive(Debug)]
pub struct FileReadTool;

impl FileReadTool {
    /// Maximum file size we'll read (10 MB).
    const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

    /// Maximum number of lines to return by default.
    const DEFAULT_MAX_LINES: usize = 2000;

    /// Maximum line length before truncation.
    const MAX_LINE_LENGTH: usize = 2000;

    /// Maximum output size in bytes (50 KB) to prevent context bloat.
    const MAX_OUTPUT_BYTES: usize = 50 * 1024;

    /// Read directory entries, sorted alphabetically with `/` suffix for subdirs.
    fn read_directory(
        path: &std::path::Path,
        display_path: &str,
        offset: usize,
        limit: usize,
    ) -> ToolResult {
        let entries = match std::fs::read_dir(path) {
            Ok(rd) => rd,
            Err(e) => return ToolResult::fail(format!("Failed to read directory: {e}")),
        };

        let mut names: Vec<String> = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => return ToolResult::fail(format!("Failed to read directory entry: {e}")),
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if is_dir {
                names.push(format!("{name}/"));
            } else {
                names.push(name);
            }
        }
        names.sort();

        let total = names.len();
        let start = if offset > 0 { offset - 1 } else { 0 };
        let end = (start + limit).min(total);

        let mut output = format!("Directory: {display_path}\n");
        if total == 0 {
            output.push_str("(empty directory)\n");
        } else {
            for (i, name) in names[start..end].iter().enumerate() {
                let idx = start + i + 1;
                output.push_str(&format!("{idx:>6}\t{name}\n"));
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("total_entries".into(), serde_json::json!(total));
        metadata.insert(
            "entries_shown".into(),
            serde_json::json!(end.saturating_sub(start)),
        );
        metadata.insert("is_directory".into(), serde_json::json!(true));

        ToolResult::ok_with_metadata(output, metadata)
    }
}

#[async_trait::async_trait]
impl BaseTool for FileReadTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file or list directory entries. Supports line ranges, \
         detects binary files, and suggests similar filenames on not-found errors."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-based)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let file_path = match args.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::fail("file_path is required"),
        };

        let offset = args
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(1);

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(Self::DEFAULT_MAX_LINES);

        let path = resolve_file_path(file_path, &ctx.working_dir);

        if let Err(msg) = validate_path_access(&path, &ctx.working_dir) {
            return ToolResult::fail(msg);
        }

        if !path.exists() {
            return ToolResult::fail(file_not_found_message(file_path, &path));
        }

        // Directory reading: list entries with optional pagination
        if path.is_dir() {
            return Self::read_directory(&path, file_path, offset, limit);
        }

        if !path.is_file() {
            return ToolResult::fail(format!("Not a file: {file_path}"));
        }

        // Check file size
        match std::fs::metadata(&path) {
            Ok(meta) => {
                if meta.len() > Self::MAX_FILE_SIZE {
                    return ToolResult::fail(format!(
                        "File too large: {} bytes (max {} bytes)",
                        meta.len(),
                        Self::MAX_FILE_SIZE
                    ));
                }
            }
            Err(e) => return ToolResult::fail(format!("Cannot read file metadata: {e}")),
        }

        // Check for binary content
        match std::fs::read(&path) {
            Ok(bytes) => {
                if is_binary_file(&path, &bytes) {
                    return ToolResult::fail(format!(
                        "Binary file detected: {file_path} ({} bytes). Use a specialized tool for binary files.",
                        bytes.len()
                    ));
                }

                let content = String::from_utf8_lossy(&bytes);
                let lines: Vec<&str> = content.lines().collect();
                let total_lines = lines.len();

                // Apply offset (1-based) and limit
                let start = if offset > 0 { offset - 1 } else { 0 };
                let end = (start + limit).min(total_lines);

                if start >= total_lines {
                    return ToolResult::fail(format!(
                        "Offset {offset} is beyond end of file ({total_lines} lines)"
                    ));
                }

                let mut output = String::new();
                let mut output_bytes: usize = 0;
                let mut lines_emitted: usize = 0;
                let mut byte_truncated = false;

                for (i, line) in lines[start..end].iter().enumerate() {
                    let line_num = start + i + 1;
                    let truncated_line = if line.len() > Self::MAX_LINE_LENGTH {
                        format!("{}...", &line[..Self::MAX_LINE_LENGTH])
                    } else {
                        line.to_string()
                    };
                    let formatted = format!("{line_num:>6}\t{truncated_line}\n");
                    let line_bytes = formatted.len();

                    if output_bytes + line_bytes > Self::MAX_OUTPUT_BYTES {
                        byte_truncated = true;
                        break;
                    }

                    output.push_str(&formatted);
                    output_bytes += line_bytes;
                    lines_emitted += 1;
                }

                // Calculate the next offset for follow-up reads.
                let next_offset = start + lines_emitted + 1;
                let has_more = next_offset <= total_lines;

                if byte_truncated {
                    let remaining = end - start - lines_emitted;
                    output.push_str(&format!(
                        "\n[...truncated: {remaining} more lines not shown (output exceeded {} KB limit). \
                         Use offset={next_offset} to continue reading.]\n",
                        Self::MAX_OUTPUT_BYTES / 1024
                    ));
                } else if end < total_lines {
                    // Lines were limited by the limit param, hint the next offset.
                    output.push_str(&format!(
                        "\n[{} more lines below. Use offset={next_offset} to continue reading.]\n",
                        total_lines - end
                    ));
                }

                // Warn if the file is potentially sensitive.
                if let Some(reason) = is_sensitive_file(&path) {
                    output.insert_str(
                        0,
                        &format!(
                            "WARNING: This is a {reason}. Do NOT include its contents \
                             in responses, commits, or logs. Treat all values as secrets.\n\n"
                        ),
                    );
                }

                let mut metadata = HashMap::new();
                metadata.insert("total_lines".into(), serde_json::json!(total_lines));
                metadata.insert("lines_shown".into(), serde_json::json!(lines_emitted));
                if has_more {
                    metadata.insert("next_offset".into(), serde_json::json!(next_offset));
                }
                if byte_truncated {
                    metadata.insert("truncated".into(), serde_json::json!(true));
                }

                ToolResult::ok_with_metadata(output, metadata)
            }
            Err(e) => ToolResult::fail(format!("Failed to read file: {e}")),
        }
    }
}

/// Build an error message for a missing file, with up to 5 suggestions from the
/// parent directory using both substring matching and Levenshtein edit distance.
fn file_not_found_message(display_path: &str, resolved: &std::path::Path) -> String {
    let mut msg = format!("File not found: {display_path}");

    let basename = match resolved.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return msg,
    };
    let basename_lower = basename.to_lowercase();

    let parent = match resolved.parent() {
        Some(p) if p.is_dir() => p,
        _ => return msg,
    };

    let entries = match std::fs::read_dir(parent) {
        Ok(rd) => rd,
        Err(_) => return msg,
    };

    // Collect candidates with a relevance score (lower is better).
    let mut scored: Vec<(String, usize)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let name_lower = name.to_lowercase();

        // Substring match gets score 0 (best)
        if name_lower.contains(&basename_lower) || basename_lower.contains(&name_lower) {
            scored.push((name, 0));
            continue;
        }

        // Levenshtein distance for typo detection
        let dist = levenshtein(&basename_lower, &name_lower);
        // Only suggest if edit distance is within 40% of the longer string length
        let max_dist = basename_lower.len().max(name_lower.len()) * 2 / 5;
        if dist <= max_dist.max(2) {
            scored.push((name, dist));
        }
    }

    if !scored.is_empty() {
        scored.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        scored.truncate(5);
        msg.push_str("\n\nDid you mean one of these?\n");
        for (s, _) in &scored {
            msg.push_str(&format!("  - {s}\n"));
        }
    }

    msg
}

/// Compute the Levenshtein edit distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    // Use single-row optimization (O(n) space).
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Known binary file extensions (fast path to avoid reading content).
const BINARY_EXTENSIONS: &[&str] = &[
    // Archives & compressed
    "zip", "gz", "tar", "bz2", "xz", "7z", "rar", "zst", "lz4", // Images
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "tiff", "tif", "avif", "heic",
    // Audio/Video
    "mp3", "mp4", "wav", "ogg", "flac", "avi", "mkv", "mov", "webm",
    // Executables & libraries
    "exe", "dll", "so", "dylib", "o", "a", "lib", "class", // Compiled/bytecode
    "pyc", "pyo", "wasm", "beam", // Databases
    "db", "sqlite", "sqlite3", // Documents (binary)
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", // Fonts
    "ttf", "otf", "woff", "woff2", "eot", // Other binary
    "bin", "dat", "pak", "jar", "war", "egg", // Serialized data
    "pb", "protobuf", "flatbuf", "msgpack", // Lock files (often large, not useful)
    "lock",
];

/// Check if a file is likely binary, first by extension, then by content inspection.
fn is_binary_file(path: &std::path::Path, bytes: &[u8]) -> bool {
    // Fast path: check extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_ascii_lowercase();
        if BINARY_EXTENSIONS.contains(&ext_lower.as_str()) {
            return true;
        }
    }
    // Content-based: check for null bytes in first 8 KB
    is_binary(bytes)
}

/// Check if content appears to be binary.
///
/// Uses two heuristics (matching OpenCode's approach):
/// 1. Null bytes in the first 8KB → definitely binary.
/// 2. More than 30% non-printable characters → likely binary.
///
/// Non-printable is defined as bytes < 9 or (> 13 and < 32), excluding
/// tab (9), newline (10), carriage return (13).
fn is_binary(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(8192);
    if check_len == 0 {
        return false;
    }
    let sample = &bytes[..check_len];

    // Check for null bytes (fast path)
    if sample.contains(&0) {
        return true;
    }

    // Check non-printable character ratio
    let non_printable = sample
        .iter()
        .filter(|&&b| b < 9 || (b > 13 && b < 32))
        .count();

    let ratio = non_printable as f64 / check_len as f64;
    ratio > 0.3
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_read_file_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let file = dir_path.join("test.txt");
        std::fs::write(&file, "line one\nline two\nline three\n").unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(&dir_path);
        let args = make_args(&[("file_path", serde_json::json!(file.to_str().unwrap()))]);
        let result = tool.execute(args, &ctx).await;

        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("line one"));
        assert!(output.contains("line two"));
        assert!(output.contains("line three"));
    }

    #[tokio::test]
    async fn test_read_file_with_offset_and_limit() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let file = dir_path.join("lines.txt");
        let content: String = (1..=10).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&file, content).unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(&dir_path);
        let args = make_args(&[
            ("file_path", serde_json::json!(file.to_str().unwrap())),
            ("offset", serde_json::json!(3)),
            ("limit", serde_json::json!(2)),
        ]);
        let result = tool.execute(args, &ctx).await;

        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("line 3"));
        assert!(output.contains("line 4"));
        assert!(!output.contains("line 5"));
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let tool = FileReadTool;
        let ctx = ToolContext::new(&dir_path);
        let args = make_args(&[(
            "file_path",
            serde_json::json!(dir_path.join("nonexistent.txt").to_str().unwrap()),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_read_binary_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let file = dir_path.join("binary.bin");
        std::fs::write(&file, &[0u8, 1, 2, 3, 0, 5]).unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(&dir_path);
        let args = make_args(&[("file_path", serde_json::json!(file.to_str().unwrap()))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Binary"));
    }

    #[tokio::test]
    async fn test_missing_file_path() {
        let tool = FileReadTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(!result.success);
    }

    #[test]
    fn test_is_binary() {
        assert!(is_binary(&[0u8, 1, 2]));
        assert!(!is_binary(b"hello world\n"));
    }

    #[test]
    fn test_is_binary_file_by_extension() {
        let path = std::path::Path::new("image.png");
        assert!(is_binary_file(path, b"this is actually text"));

        let path = std::path::Path::new("archive.zip");
        assert!(is_binary_file(path, b"text content"));

        let path = std::path::Path::new("data.sqlite3");
        assert!(is_binary_file(path, b"text content"));
    }

    #[test]
    fn test_is_binary_file_case_insensitive() {
        let path = std::path::Path::new("image.PNG");
        assert!(is_binary_file(path, b"text"));

        let path = std::path::Path::new("image.Jpg");
        assert!(is_binary_file(path, b"text"));
    }

    #[test]
    fn test_is_binary_file_text_extensions_use_content() {
        // .rs file with no null bytes = not binary
        let path = std::path::Path::new("main.rs");
        assert!(!is_binary_file(path, b"fn main() {}"));

        // .rs file with null bytes = binary
        let path = std::path::Path::new("main.rs");
        assert!(is_binary_file(path, &[0u8, 1, 2]));
    }

    #[test]
    fn test_is_binary_file_no_extension() {
        // No extension, fallback to content check
        let path = std::path::Path::new("Makefile");
        assert!(!is_binary_file(path, b"all: build"));
        assert!(is_binary_file(path, &[0u8]));
    }

    #[tokio::test]
    async fn test_read_directory() {
        let tmp = TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();
        fs::write(tmp_path.join("alpha.rs"), "").unwrap();
        fs::write(tmp_path.join("beta.txt"), "").unwrap();
        fs::create_dir(tmp_path.join("gamma")).unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(tmp_path.to_str().unwrap());
        let args = make_args(&[("file_path", serde_json::json!(tmp_path.to_str().unwrap()))]);
        let result = tool.execute(args, &ctx).await;

        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("alpha.rs"));
        assert!(output.contains("beta.txt"));
        assert!(output.contains("gamma/"));
        // Verify sorted order: alpha < beta < gamma
        let alpha_pos = output.find("alpha.rs").unwrap();
        let beta_pos = output.find("beta.txt").unwrap();
        let gamma_pos = output.find("gamma/").unwrap();
        assert!(alpha_pos < beta_pos);
        assert!(beta_pos < gamma_pos);

        let meta = &result.metadata;
        assert_eq!(meta["total_entries"], 3);
        assert_eq!(meta["is_directory"], true);
    }

    #[tokio::test]
    async fn test_read_directory_with_pagination() {
        let tmp = TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();
        for name in ["aaa", "bbb", "ccc", "ddd", "eee"] {
            fs::write(tmp_path.join(name), "").unwrap();
        }

        let tool = FileReadTool;
        let ctx = ToolContext::new(tmp_path.to_str().unwrap());
        let args = make_args(&[
            ("file_path", serde_json::json!(tmp_path.to_str().unwrap())),
            ("offset", serde_json::json!(2)),
            ("limit", serde_json::json!(2)),
        ]);
        let result = tool.execute(args, &ctx).await;

        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("bbb"));
        assert!(output.contains("ccc"));
        assert!(!output.contains("aaa"));
        assert!(!output.contains("ddd"));

        let meta = &result.metadata;
        assert_eq!(meta["total_entries"], 5);
        assert_eq!(meta["entries_shown"], 2);
    }

    #[tokio::test]
    async fn test_read_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(tmp_path.to_str().unwrap());
        let args = make_args(&[("file_path", serde_json::json!(tmp_path.to_str().unwrap()))]);
        let result = tool.execute(args, &ctx).await;

        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("(empty directory)"));

        let meta = &result.metadata;
        assert_eq!(meta["total_entries"], 0);
    }

    #[tokio::test]
    async fn test_file_not_found_suggestions_levenshtein() {
        let tmp = TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();
        fs::write(tmp_path.join("file.rs"), "").unwrap();
        fs::write(tmp_path.join("file_edit.rs"), "").unwrap();
        fs::write(tmp_path.join("other.txt"), "").unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(tmp_path.to_str().unwrap());
        // "flie.rs" is a typo for "file.rs" — Levenshtein distance = 2
        let wrong_path = tmp_path.join("flie.rs");
        let args = make_args(&[("file_path", serde_json::json!(wrong_path.to_str().unwrap()))]);
        let result = tool.execute(args, &ctx).await;

        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("not found"));
        assert!(
            err.contains("Did you mean"),
            "Should suggest similar files, got: {err}"
        );
        assert!(
            err.contains("file.rs"),
            "Should suggest file.rs for typo flie.rs, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_file_not_found_suggestions_substring() {
        let tmp = TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();
        fs::write(tmp_path.join("file_read.rs"), "").unwrap();
        fs::write(tmp_path.join("file_write.rs"), "").unwrap();
        fs::write(tmp_path.join("other.txt"), "").unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(tmp_path.to_str().unwrap());
        // "file" is contained in "file_read.rs" and "file_write.rs"
        let wrong_path = tmp_path.join("file");
        let args = make_args(&[("file_path", serde_json::json!(wrong_path.to_str().unwrap()))]);
        let result = tool.execute(args, &ctx).await;

        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("Did you mean"));
        assert!(err.contains("file_read.rs"));
        assert!(err.contains("file_write.rs"));
        assert!(!err.contains("other.txt"));
    }

    // ---- Next offset hint ----

    #[tokio::test]
    async fn test_read_next_offset_hint() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let file = dir_path.join("lines.txt");
        let content: String = (1..=20).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&file, content).unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(&dir_path);
        // Read only 5 lines from the start
        let args = make_args(&[
            ("file_path", serde_json::json!(file.to_str().unwrap())),
            ("limit", serde_json::json!(5)),
        ]);
        let result = tool.execute(args, &ctx).await;

        assert!(result.success);
        let output = result.output.unwrap();
        // Should hint next offset
        assert!(
            output.contains("offset=6"),
            "Should hint offset=6, got: {output}"
        );
        assert!(output.contains("15 more lines below"));
        // Metadata should have next_offset
        assert_eq!(
            result.metadata.get("next_offset"),
            Some(&serde_json::json!(6))
        );
    }

    #[tokio::test]
    async fn test_read_no_next_offset_at_end() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let file = dir_path.join("small.txt");
        std::fs::write(&file, "line 1\nline 2\nline 3\n").unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(&dir_path);
        let args = make_args(&[("file_path", serde_json::json!(file.to_str().unwrap()))]);
        let result = tool.execute(args, &ctx).await;

        assert!(result.success);
        // No next_offset when all lines are shown
        assert!(result.metadata.get("next_offset").is_none());
    }

    // ---- Output byte limit ----

    #[tokio::test]
    async fn test_read_large_file_byte_truncation() {
        let tmp = TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();

        // Create a file with very long lines that exceed 50KB output
        let long_line = "x".repeat(500);
        let content: String = (0..200)
            .map(|_| long_line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(tmp_path.join("big.txt"), &content).unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(&tmp_path);
        let args = make_args(&[(
            "file_path",
            serde_json::json!(tmp_path.join("big.txt").to_str().unwrap()),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);

        let output = result.output.unwrap();
        // Output should be capped around 50KB
        assert!(output.len() <= FileReadTool::MAX_OUTPUT_BYTES + 200); // some margin for truncation message
        if content.len() > FileReadTool::MAX_OUTPUT_BYTES {
            assert!(output.contains("truncated"));
            assert_eq!(
                result.metadata.get("truncated"),
                Some(&serde_json::json!(true))
            );
        }
    }

    // ---- Binary detection improvements ----

    #[test]
    fn test_binary_detection_null_bytes() {
        let bytes = b"hello\x00world";
        assert!(is_binary(bytes));
    }

    #[test]
    fn test_binary_detection_high_non_printable_ratio() {
        // 50% non-printable chars (bytes < 9)
        let mut bytes = vec![0x01u8; 50];
        bytes.extend_from_slice(&[b'a'; 50]);
        assert!(is_binary(&bytes));
    }

    #[test]
    fn test_binary_detection_low_non_printable_ratio() {
        // Mostly printable with a few control chars (< 30%)
        let mut bytes = vec![b'a'; 100];
        bytes[0] = 0x01;
        bytes[1] = 0x02;
        assert!(!is_binary(&bytes));
    }

    #[test]
    fn test_binary_detection_empty() {
        assert!(!is_binary(&[]));
    }

    #[test]
    fn test_binary_detection_text_with_tabs_newlines() {
        // Tabs and newlines should NOT count as non-printable
        let bytes = b"hello\tworld\nfoo\rbar";
        assert!(!is_binary(bytes));
    }

    // ---- Sensitive file warning ----

    #[tokio::test]
    async fn test_read_sensitive_env_file_warns() {
        let tmp = TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();
        let env_file = tmp_path.join(".env");
        fs::write(&env_file, "SECRET_KEY=abc123\nDB_PASSWORD=hunter2\n").unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(&tmp_path);
        let args = make_args(&[("file_path", serde_json::json!(env_file.to_str().unwrap()))]);
        let result = tool.execute(args, &ctx).await;

        assert!(result.success);
        let output = result.output.unwrap();
        assert!(
            output.starts_with("WARNING:"),
            "Sensitive file should have WARNING prefix, got: {}",
            &output[..output.len().min(100)]
        );
        assert!(output.contains("secrets"));
    }

    // ---- Levenshtein distance ----

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn test_levenshtein_transposition() {
        // "flie" vs "file" = 2 (swap i and l requires delete + insert)
        assert_eq!(levenshtein("flie", "file"), 2);
    }

    #[test]
    fn test_levenshtein_single_edit() {
        assert_eq!(levenshtein("cat", "car"), 1); // substitution
        assert_eq!(levenshtein("cat", "cats"), 1); // insertion
        assert_eq!(levenshtein("cats", "cat"), 1); // deletion
    }

    #[test]
    fn test_levenshtein_completely_different() {
        assert_eq!(levenshtein("abc", "xyz"), 3);
    }

    #[tokio::test]
    async fn test_read_env_example_no_warning() {
        let tmp = TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();
        let env_file = tmp_path.join(".env.example");
        fs::write(&env_file, "SECRET_KEY=\nDB_PASSWORD=\n").unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(&tmp_path);
        let args = make_args(&[("file_path", serde_json::json!(env_file.to_str().unwrap()))]);
        let result = tool.execute(args, &ctx).await;

        assert!(result.success);
        let output = result.output.unwrap();
        assert!(
            !output.starts_with("WARNING:"),
            ".env.example should NOT have warning"
        );
    }
}
