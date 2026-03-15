//! Write file tool — writes content to a file with atomic writes and directory creation.

use std::collections::HashMap;
use std::path::Path;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::formatter;
use crate::path_utils::{resolve_file_path, validate_path_access};

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

        if let Err(e) = std::fs::rename(&tmp_path, path) {
            // Clean up temp file on rename failure
            let _ = std::fs::remove_file(&tmp_path);
            return ToolResult::fail(format!("Failed to rename temp file: {e}"));
        }

        // Auto-format if a formatter is available
        let formatted = formatter::format_file(file_path, &ctx.working_dir);

        let lines = content.lines().count();
        let bytes = content.len();

        let mut metadata = HashMap::new();
        metadata.insert("lines".into(), serde_json::json!(lines));
        metadata.insert("bytes".into(), serde_json::json!(bytes));
        if formatted {
            metadata.insert("formatted".into(), serde_json::json!(true));
        }

        let fmt_note = if formatted { " (formatted)" } else { "" };
        ToolResult::ok_with_metadata(
            format!("Wrote {bytes} bytes ({lines} lines) to {file_path}{fmt_note}"),
            metadata,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_write_file_basic() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");

        let tool = FileWriteTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("content", serde_json::json!("hello\nworld\n")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "hello\nworld\n"
        );
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("a/b/c/test.txt");

        let tool = FileWriteTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("content", serde_json::json!("nested content")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "nested content"
        );
    }

    #[tokio::test]
    async fn test_write_no_create_dirs() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("nonexistent/test.txt");

        let tool = FileWriteTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("content", serde_json::json!("content")),
            ("create_dirs", serde_json::json!(false)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_write_overwrites_existing() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "old content").unwrap();

        let tool = FileWriteTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("content", serde_json::json!("new content")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "new content");
    }
}
