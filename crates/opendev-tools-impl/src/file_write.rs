//! Write file tool — writes content to a file with atomic writes and directory creation.

use std::collections::HashMap;
use std::path::Path;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::diagnostics_helper;
use crate::formatter;
use crate::path_utils::{is_sensitive_file, resolve_file_path, validate_path_access};

/// Tool for writing file contents.
#[derive(Debug)]
pub struct FileWriteTool;

#[async_trait::async_trait]
impl BaseTool for FileWriteTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates parent directories if needed. Uses atomic writes."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                },
                "create_dirs": {
                    "type": "boolean",
                    "description": "Create parent directories if they don't exist (default: true)"
                }
            },
            "required": ["file_path", "content"]
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

        let content = match args.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::fail("content is required"),
        };

        let create_dirs = args
            .get("create_dirs")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let path = resolve_file_path(file_path, &ctx.working_dir);

        if let Err(msg) = validate_path_access(&path, &ctx.working_dir) {
            return ToolResult::fail(msg);
        }

        // Warn about writing to sensitive files.
        if let Some(reason) = is_sensitive_file(&path) {
            return ToolResult::fail(format!(
                "Refusing to write to {}: {} — this file likely contains secrets. \
                 If you need to modify it, ask the user to do so manually.",
                file_path, reason
            ));
        }

        // Create parent directories if needed
        if create_dirs {
            if let Some(parent) = path.parent()
                && !parent.exists()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                return ToolResult::fail(format!("Failed to create directories: {e}"));
            }
        } else if let Some(parent) = path.parent()
            && !parent.exists()
        {
            return ToolResult::fail(format!(
                "Parent directory does not exist: {}",
                parent.display()
            ));
        }

        // Atomic write: write to temp file then rename
        let dir = path.parent().unwrap_or(Path::new("."));
        let tmp_path = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));

        if let Err(e) = std::fs::write(&tmp_path, content) {
            return ToolResult::fail(format!("Failed to write temp file: {e}"));
        }

        if let Err(e) = std::fs::rename(&tmp_path, &path) {
            // Clean up temp file on rename failure
            let _ = std::fs::remove_file(&tmp_path);
            return ToolResult::fail(format!("Failed to rename temp file: {e}"));
        }

        // Auto-format if a formatter is available
        let formatted =
            formatter::format_file(path.to_str().unwrap_or(file_path), &ctx.working_dir);

        let lines = content.lines().count();
        let bytes = content.len();

        let mut metadata = HashMap::new();
        metadata.insert("lines".into(), serde_json::json!(lines));
        metadata.insert("bytes".into(), serde_json::json!(bytes));
        if formatted {
            metadata.insert("formatted".into(), serde_json::json!(true));
        }

        let fmt_note = if formatted { " (formatted)" } else { "" };
        let mut output = format!("Wrote {bytes} bytes ({lines} lines) to {file_path}{fmt_note}");

        // Collect LSP diagnostics after write
        if let Some(diag_output) =
            diagnostics_helper::collect_post_edit_diagnostics(ctx, &path).await
        {
            output.push_str(&diag_output);
        }

        ToolResult::ok_with_metadata(output, metadata)
    }
}

#[cfg(test)]
#[path = "file_write_tests.rs"]
mod tests;
