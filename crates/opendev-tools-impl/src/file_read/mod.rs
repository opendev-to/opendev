//! Read file tool — reads file contents with optional line ranges and binary detection.

mod binary;
mod suggestions;

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::path_utils::{is_sensitive_file, resolve_file_path};

use binary::is_binary_file;
use suggestions::file_not_found_message;

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
        "Read"
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

    fn is_read_only(&self, _args: &HashMap<String, serde_json::Value>) -> bool {
        true
    }

    fn is_concurrent_safe(&self, _args: &HashMap<String, serde_json::Value>) -> bool {
        true
    }

    fn category(&self) -> opendev_tools_core::ToolCategory {
        opendev_tools_core::ToolCategory::Read
    }

    fn truncation_rule(&self) -> Option<opendev_tools_core::TruncationRule> {
        Some(opendev_tools_core::TruncationRule::head(15000))
    }

    fn search_hint(&self) -> Option<&str> {
        Some("read file contents by path with line ranges")
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

#[cfg(test)]
mod tests;
