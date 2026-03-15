//! Edit file tool — string replacement with 9-pass fuzzy matching, diff preview,
//! per-file locking, and proper line-count statistics.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::diagnostics_helper;
use crate::edit_replacers;
use crate::formatter;
use crate::path_utils::{is_sensitive_file, resolve_file_path, validate_path_access};

// ---------------------------------------------------------------------------
// Per-file locking: serialize concurrent edits to the same file.
// ---------------------------------------------------------------------------

static FILE_LOCKS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn get_file_lock(path: &Path) -> Arc<Mutex<()>> {
    let mut map = FILE_LOCKS.lock().unwrap();
    map.entry(path.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

// ---------------------------------------------------------------------------
// FileEditTool
// ---------------------------------------------------------------------------

/// Tool for editing files via string replacement with fuzzy matching fallback.
#[derive(Debug)]
pub struct FileEditTool;

#[async_trait::async_trait]
impl BaseTool for FileEditTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing a string match. Uses a 9-pass fuzzy matching \
         chain so minor whitespace/indentation differences are tolerated. \
         The old_string must be unique in the file unless replace_all is true."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The string to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement string"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default: false)"
                }
            },
            "required": ["file_path", "old_string", "new_string"]
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
        let old_string = match args.get("old_string").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::fail("old_string is required"),
        };
        let new_string = match args.get("new_string").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::fail("new_string is required"),
        };
        let replace_all = args
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if old_string == new_string {
            return ToolResult::fail("old_string and new_string are identical");
        }

        let path = resolve_file_path(file_path, &ctx.working_dir);

        if let Err(msg) = validate_path_access(&path, &ctx.working_dir) {
            return ToolResult::fail(msg);
        }

        if !path.exists() {
            return ToolResult::fail(format!("File not found: {file_path}"));
        }

        // Block editing sensitive files (same as file_write).
        if let Some(reason) = is_sensitive_file(&path) {
            return ToolResult::fail(format!(
                "Refusing to edit {}: {} — this file likely contains secrets. \
                 If you need to modify it, ask the user to do so manually.",
                file_path, reason
            ));
        }

        // Acquire per-file lock — scoped so the guard drops before async diagnostics
        let (output_text, metadata) = {
            let lock = get_file_lock(&path);
            let _guard = lock.lock().unwrap();

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => return ToolResult::fail(format!("Failed to read file: {e}")),
            };

            // --- Fuzzy match ---
            let (actual_old, pass_name) = match edit_replacers::find_match(&content, old_string) {
                Some(m) => (m.actual, m.pass_name),
                None => {
                    return ToolResult::fail(format!(
                        "old_string not found in {file_path}. Make sure the string matches \
                         the file content (tried 9 fuzzy matching passes)."
                    ));
                }
            };

            // --- Uniqueness check ---
            let count = content.matches(&actual_old as &str).count();

            if count > 1 && !replace_all {
                let positions = edit_replacers::find_occurrence_positions(&content, &actual_old);
                let locations: String = positions
                    .iter()
                    .map(|n| format!("line {n}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return ToolResult::fail(format!(
                    "old_string found {count} times at {locations} in {file_path}. \
                     Provide more surrounding context to make the match unique, \
                     or use replace_all=true."
                ));
            }

            // --- Perform replacement ---
            let new_content = if replace_all {
                content.replace(&actual_old, new_string)
            } else {
                content.replacen(&actual_old, new_string, 1)
            };

            // --- Diff stats ---
            let old_line_parts: Vec<&str> = actual_old.split('\n').collect();
            let new_line_parts: Vec<&str> = new_string.split('\n').collect();
            let removals = old_line_parts.len();
            let additions = new_line_parts.len();

            // --- Generate unified diff preview ---
            let diff_text = edit_replacers::unified_diff(file_path, &content, &new_content, 3);

            // --- Atomic write ---
            let dir = path.parent().unwrap_or(Path::new("."));
            let tmp_path = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));

            if let Err(e) = std::fs::write(&tmp_path, &new_content) {
                return ToolResult::fail(format!("Failed to write temp file: {e}"));
            }
            if let Err(e) = std::fs::rename(&tmp_path, &path) {
                let _ = std::fs::remove_file(&tmp_path);
                return ToolResult::fail(format!("Failed to rename temp file: {e}"));
            }

            // Auto-format if a formatter is available
            let formatted = formatter::format_file(file_path, &ctx.working_dir);

            let replacements = if replace_all { count } else { 1 };

            let mut metadata = HashMap::new();
            metadata.insert("replacements".into(), serde_json::json!(replacements));
            metadata.insert("additions".into(), serde_json::json!(additions));
            metadata.insert("removals".into(), serde_json::json!(removals));
            metadata.insert("diff".into(), serde_json::json!(diff_text));
            if pass_name != "simple" {
                metadata.insert("match_pass".into(), serde_json::json!(pass_name));
            }
            if formatted {
                metadata.insert("formatted".into(), serde_json::json!(true));
            }

            let fmt_note = if formatted { " (formatted)" } else { "" };
            let summary = format!(
                "Edited {file_path}: {replacements} replacement(s), \
                 {additions} addition(s) and {removals} removal(s){fmt_note}"
            );
            let output_text = if diff_text.is_empty() {
                summary
            } else {
                format!("{summary}\n{diff_text}")
            };

            (output_text, metadata)
        }; // lock guard dropped here

        // Collect LSP diagnostics after edit (requires no lock held)
        let mut output_text = output_text;
        if let Some(diag_output) =
            diagnostics_helper::collect_post_edit_diagnostics(ctx, &path).await
        {
            output_text.push_str(&diag_output);
        }

        ToolResult::ok_with_metadata(output_text, metadata)
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
    async fn test_edit_single_replacement() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.rs");
        std::fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("old_string", serde_json::json!("hello")),
            ("new_string", serde_json::json!("world")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("world"));
        assert!(!content.contains("hello"));
    }

    #[tokio::test]
    async fn test_edit_not_unique_fails() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "foo bar foo baz foo").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("old_string", serde_json::json!("foo")),
            ("new_string", serde_json::json!("qux")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("3 times"));
        assert!(err.contains("line ")); // occurrence locations reported
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "foo bar foo baz foo").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("old_string", serde_json::json!("foo")),
            ("new_string", serde_json::json!("qux")),
            ("replace_all", serde_json::json!(true)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "qux bar qux baz qux");
    }

    #[tokio::test]
    async fn test_edit_not_found() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("old_string", serde_json::json!("nonexistent")),
            ("new_string", serde_json::json!("replacement")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_edit_same_string() {
        let tool = FileEditTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[
            ("file_path", serde_json::json!("/tmp/test.txt")),
            ("old_string", serde_json::json!("same")),
            ("new_string", serde_json::json!("same")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("identical"));
    }

    #[tokio::test]
    async fn test_edit_fuzzy_whitespace() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n    let x = 1;\n    let y = 2;\n}\n",
        )
        .unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());
        // LLM provides without indentation
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("old_string", serde_json::json!("let x = 1;\nlet y = 2;")),
            (
                "new_string",
                serde_json::json!("    let x = 10;\n    let y = 20;"),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(
            result.success,
            "fuzzy match should succeed: {:?}",
            result.error
        );
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("let x = 10;"));
        assert!(content.contains("let y = 20;"));
        // Should report the match pass used
        if let Some(pass) = result.metadata.get("match_pass") {
            assert!(pass.as_str().unwrap() != "simple");
        }
    }

    #[tokio::test]
    async fn test_edit_diff_preview_in_metadata() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("old_string", serde_json::json!("line2")),
            ("new_string", serde_json::json!("line2_modified")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let diff = result.metadata.get("diff").unwrap().as_str().unwrap();
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+line2_modified"));
    }

    #[tokio::test]
    async fn test_edit_trailing_newline_counts() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "hello\n").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());
        // Replace "hello\n" with "hello" — removes trailing newline
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("old_string", serde_json::json!("hello\n")),
            ("new_string", serde_json::json!("hello")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        // "hello\n" split by \n = ["hello", ""] = 2 parts
        // "hello" split by \n = ["hello"] = 1 part
        let removals = result.metadata.get("removals").unwrap().as_u64().unwrap();
        let additions = result.metadata.get("additions").unwrap().as_u64().unwrap();
        assert_eq!(removals, 2);
        assert_eq!(additions, 1);
    }

    #[tokio::test]
    async fn test_edit_occurrence_locations_reported() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "foo\nbar\nfoo\nbaz\nfoo\n").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("old_string", serde_json::json!("foo")),
            ("new_string", serde_json::json!("qux")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("line 1"));
        assert!(err.contains("line 3"));
        assert!(err.contains("line 5"));
    }

    #[tokio::test]
    async fn test_edit_concurrent_same_file() {
        // Verify that concurrent edits to the same file serialize correctly
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("concurrent.txt");
        std::fs::write(&file_path, "aaa bbb ccc").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());

        // First edit
        let args1 = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("old_string", serde_json::json!("aaa")),
            ("new_string", serde_json::json!("xxx")),
        ]);
        let r1 = tool.execute(args1, &ctx).await;
        assert!(r1.success);

        // Second edit on the modified file
        let args2 = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            ("old_string", serde_json::json!("bbb")),
            ("new_string", serde_json::json!("yyy")),
        ]);
        let r2 = tool.execute(args2, &ctx).await;
        assert!(r2.success);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "xxx yyy ccc");
    }

    #[tokio::test]
    async fn test_edit_rejects_sensitive_file() {
        let tmp = TempDir::new().unwrap();
        let env_file = tmp.path().join(".env");
        std::fs::write(&env_file, "SECRET=abc123\n").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(env_file.to_str().unwrap())),
            ("old_string", serde_json::json!("abc123")),
            ("new_string", serde_json::json!("newvalue")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("secrets"), "Should mention secrets: {err}");
        // File should be unchanged
        assert_eq!(
            std::fs::read_to_string(&env_file).unwrap(),
            "SECRET=abc123\n"
        );
    }

    #[tokio::test]
    async fn test_edit_allows_env_example() {
        let tmp = TempDir::new().unwrap();
        let env_file = tmp.path().join(".env.example");
        std::fs::write(&env_file, "SECRET=placeholder\n").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(env_file.to_str().unwrap())),
            ("old_string", serde_json::json!("placeholder")),
            ("new_string", serde_json::json!("your_value_here")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(
            result.success,
            ".env.example should be editable: {:?}",
            result.error
        );
    }
}
