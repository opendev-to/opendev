//! Multi-edit tool — apply multiple sequential edits to a single file atomically.
//!
//! Instead of calling `edit_file` N times (each reading/writing the file), this
//! tool reads the file once, applies all edits in-memory in order, writes the
//! result atomically, and returns a single combined diff.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::edit_replacers;
use crate::formatter;
use crate::path_utils::resolve_file_path;

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
// MultiEditTool
// ---------------------------------------------------------------------------

/// Tool for applying multiple sequential edits to a single file atomically.
#[derive(Debug)]
pub struct MultiEditTool;

/// A single edit operation within a multi-edit batch.
struct EditOp {
    old_string: String,
    new_string: String,
    replace_all: bool,
}

#[async_trait::async_trait]
impl BaseTool for MultiEditTool {
    fn name(&self) -> &str {
        "multi_edit"
    }

    fn description(&self) -> &str {
        "Apply multiple sequential edits to a single file atomically. \
         The file is read once, all edits are applied in order in memory, \
         then written back in a single atomic operation. Each edit uses the \
         same 9-pass fuzzy matching as edit_file."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit"
                },
                "edits": {
                    "type": "array",
                    "description": "Array of edit operations to apply sequentially",
                    "items": {
                        "type": "object",
                        "properties": {
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
                        "required": ["old_string", "new_string"]
                    }
                }
            },
            "required": ["file_path", "edits"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        // --- Parse arguments ---
        let file_path = match args.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::fail("file_path is required"),
        };

        let edits_val = match args.get("edits").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => return ToolResult::fail("edits is required and must be an array"),
        };

        if edits_val.is_empty() {
            return ToolResult::fail("edits array must not be empty");
        }

        // Parse each edit operation
        let mut edits = Vec::with_capacity(edits_val.len());
        for (i, edit_val) in edits_val.iter().enumerate() {
            let old_string = match edit_val.get("old_string").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => {
                    return ToolResult::fail(format!("edit[{i}]: old_string is required"));
                }
            };
            let new_string = match edit_val.get("new_string").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => {
                    return ToolResult::fail(format!("edit[{i}]: new_string is required"));
                }
            };
            let replace_all = edit_val
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if old_string == new_string {
                return ToolResult::fail(format!(
                    "edit[{i}]: old_string and new_string are identical"
                ));
            }

            edits.push(EditOp {
                old_string,
                new_string,
                replace_all,
            });
        }

        // --- Resolve path and check existence ---
        let path = resolve_file_path(file_path, &ctx.working_dir);
        if !path.exists() {
            return ToolResult::fail(format!("File not found: {file_path}"));
        }

        // --- Acquire per-file lock ---
        let lock = get_file_lock(&path);
        let _guard = lock.lock().unwrap();

        // --- Read file once ---
        let original_content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to read file: {e}")),
        };

        // --- Apply edits sequentially in memory ---
        let mut content = original_content.clone();
        let mut total_additions: usize = 0;
        let mut total_removals: usize = 0;
        let mut total_replacements: usize = 0;
        let mut edit_summaries: Vec<String> = Vec::new();

        for (i, edit) in edits.iter().enumerate() {
            // Fuzzy match against current in-memory content
            let (actual_old, _pass_name) =
                match edit_replacers::find_match(&content, &edit.old_string) {
                    Some(m) => (m.actual, m.pass_name),
                    None => {
                        return ToolResult::fail(format!(
                            "edit[{i}]: old_string not found in {file_path}. \
                             Make sure the string matches the file content \
                             (tried 9 fuzzy matching passes). \
                             Note: earlier edits in this batch may have changed the content."
                        ));
                    }
                };

            // Uniqueness check
            let count = content.matches(&actual_old as &str).count();
            if count > 1 && !edit.replace_all {
                let positions = edit_replacers::find_occurrence_positions(&content, &actual_old);
                let locations: String = positions
                    .iter()
                    .map(|n| format!("line {n}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return ToolResult::fail(format!(
                    "edit[{i}]: old_string found {count} times at {locations} in {file_path}. \
                     Provide more surrounding context to make the match unique, \
                     or use replace_all=true."
                ));
            }

            // Perform replacement
            let new_content = if edit.replace_all {
                content.replace(&actual_old, &edit.new_string)
            } else {
                content.replacen(&actual_old, &edit.new_string, 1)
            };

            // Track stats
            let old_line_parts: Vec<&str> = actual_old.split('\n').collect();
            let new_line_parts: Vec<&str> = edit.new_string.split('\n').collect();
            let removals = old_line_parts.len();
            let additions = new_line_parts.len();
            let replacements = if edit.replace_all { count } else { 1 };

            total_additions += additions;
            total_removals += removals;
            total_replacements += replacements;

            edit_summaries.push(format!(
                "edit[{i}]: {replacements} replacement(s), +{additions}/-{removals} lines"
            ));

            content = new_content;
        }

        // --- Generate combined diff ---
        let diff_text = edit_replacers::unified_diff(file_path, &original_content, &content, 3);

        // --- Atomic write ---
        let dir = path.parent().unwrap_or(Path::new("."));
        let tmp_path = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));

        if let Err(e) = std::fs::write(&tmp_path, &content) {
            return ToolResult::fail(format!("Failed to write temp file: {e}"));
        }
        if let Err(e) = std::fs::rename(&tmp_path, &path) {
            let _ = std::fs::remove_file(&tmp_path);
            return ToolResult::fail(format!("Failed to rename temp file: {e}"));
        }

        // --- Auto-format ---
        let formatted = formatter::format_file(file_path, &ctx.working_dir);

        // --- Build result ---
        let mut metadata = HashMap::new();
        metadata.insert(
            "total_replacements".into(),
            serde_json::json!(total_replacements),
        );
        metadata.insert("total_additions".into(), serde_json::json!(total_additions));
        metadata.insert("total_removals".into(), serde_json::json!(total_removals));
        metadata.insert("edits_applied".into(), serde_json::json!(edits.len()));
        metadata.insert("diff".into(), serde_json::json!(diff_text));
        if formatted {
            metadata.insert("formatted".into(), serde_json::json!(true));
        }

        let fmt_note = if formatted { " (formatted)" } else { "" };
        let summary = format!(
            "Applied {} edit(s) to {file_path}: {total_replacements} total replacement(s), \
             {total_additions} addition(s) and {total_removals} removal(s){fmt_note}",
            edits.len()
        );

        let details = edit_summaries.join("\n");
        let output_text = if diff_text.is_empty() {
            format!("{summary}\n{details}")
        } else {
            format!("{summary}\n{details}\n{diff_text}")
        };

        ToolResult::ok_with_metadata(output_text, metadata)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

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
    async fn test_multi_edit_two_replacements() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n    let x = 1;\n    let y = 2;\n}\n",
        )
        .unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            (
                "edits",
                serde_json::json!([
                    { "old_string": "let x = 1;", "new_string": "let x = 10;" },
                    { "old_string": "let y = 2;", "new_string": "let y = 20;" }
                ]),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success, "expected success: {:?}", result.error);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("let x = 10;"));
        assert!(content.contains("let y = 20;"));
        assert!(!content.contains("let x = 1;"));
        assert!(!content.contains("let y = 2;"));

        // Check metadata
        assert_eq!(
            result.metadata.get("edits_applied").unwrap(),
            &serde_json::json!(2)
        );
    }

    #[tokio::test]
    async fn test_multi_edit_sequential_dependency() {
        // Second edit depends on the result of the first
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "aaa bbb ccc").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            (
                "edits",
                serde_json::json!([
                    { "old_string": "aaa", "new_string": "xxx" },
                    { "old_string": "xxx bbb", "new_string": "yyy zzz" }
                ]),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success, "expected success: {:?}", result.error);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "yyy zzz ccc");
    }

    #[tokio::test]
    async fn test_multi_edit_empty_edits_fails() {
        let tool = MultiEditTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[
            ("file_path", serde_json::json!("/tmp/test.txt")),
            ("edits", serde_json::json!([])),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_multi_edit_identical_old_new_fails() {
        let tool = MultiEditTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[
            ("file_path", serde_json::json!("/tmp/test.txt")),
            (
                "edits",
                serde_json::json!([
                    { "old_string": "same", "new_string": "same" }
                ]),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("identical"));
    }

    #[tokio::test]
    async fn test_multi_edit_not_found_fails() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            (
                "edits",
                serde_json::json!([
                    { "old_string": "nonexistent", "new_string": "replacement" }
                ]),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_multi_edit_second_edit_fails_no_write() {
        // If the second edit fails, the file should NOT be modified
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "aaa bbb ccc").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            (
                "edits",
                serde_json::json!([
                    { "old_string": "aaa", "new_string": "xxx" },
                    { "old_string": "nonexistent", "new_string": "yyy" }
                ]),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);

        // File should be unchanged because edits are atomic
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "aaa bbb ccc");
    }

    #[tokio::test]
    async fn test_multi_edit_not_unique_fails() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "foo bar foo baz foo").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            (
                "edits",
                serde_json::json!([
                    { "old_string": "foo", "new_string": "qux" }
                ]),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("3 times"));
    }

    #[tokio::test]
    async fn test_multi_edit_replace_all() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "foo bar foo baz foo").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            (
                "edits",
                serde_json::json!([
                    { "old_string": "foo", "new_string": "qux", "replace_all": true }
                ]),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success, "expected success: {:?}", result.error);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "qux bar qux baz qux");
    }

    #[tokio::test]
    async fn test_multi_edit_diff_in_metadata() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            (
                "edits",
                serde_json::json!([
                    { "old_string": "line1", "new_string": "LINE1" },
                    { "old_string": "line3", "new_string": "LINE3" }
                ]),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let diff = result.metadata.get("diff").unwrap().as_str().unwrap();
        assert!(diff.contains("-line1"));
        assert!(diff.contains("+LINE1"));
        assert!(diff.contains("-line3"));
        assert!(diff.contains("+LINE3"));
    }

    #[tokio::test]
    async fn test_multi_edit_file_not_found() {
        let tmp = TempDir::new().unwrap();
        let tool = MultiEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            (
                "file_path",
                serde_json::json!(tmp.path().join("nonexistent.txt").to_str().unwrap()),
            ),
            (
                "edits",
                serde_json::json!([
                    { "old_string": "a", "new_string": "b" }
                ]),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_multi_edit_missing_file_path() {
        let tool = MultiEditTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "edits",
            serde_json::json!([
                { "old_string": "a", "new_string": "b" }
            ]),
        )]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("file_path is required"));
    }

    #[tokio::test]
    async fn test_multi_edit_missing_edits() {
        let tool = MultiEditTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[("file_path", serde_json::json!("/tmp/test.txt"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("edits is required"));
    }

    #[tokio::test]
    async fn test_multi_edit_three_edits() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "alpha\nbeta\ngamma\ndelta\n").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("file_path", serde_json::json!(file_path.to_str().unwrap())),
            (
                "edits",
                serde_json::json!([
                    { "old_string": "alpha", "new_string": "ALPHA" },
                    { "old_string": "gamma", "new_string": "GAMMA" },
                    { "old_string": "delta", "new_string": "DELTA" }
                ]),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success, "expected success: {:?}", result.error);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "ALPHA\nbeta\nGAMMA\nDELTA\n");

        assert_eq!(
            result.metadata.get("edits_applied").unwrap(),
            &serde_json::json!(3)
        );
    }
}
