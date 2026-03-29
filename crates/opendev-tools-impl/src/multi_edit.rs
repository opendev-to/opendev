//! Multi-edit tool — apply multiple sequential edits to a single file atomically.
//!
//! Instead of calling `edit_file` N times (each reading/writing the file), this
//! tool reads the file once, applies all edits in-memory in order, writes the
//! result atomically, and returns a single combined diff.

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
                                "description": "The string to find and replace. Must be different from new_string"
                            },
                            "new_string": {
                                "type": "string",
                                "description": "The replacement string. Must be different from old_string"
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
                continue;
            }

            edits.push(EditOp {
                old_string,
                new_string,
                replace_all,
            });
        }

        // --- Resolve path and check existence ---
        let path = resolve_file_path(file_path, &ctx.working_dir);

        if let Err(msg) = validate_path_access(&path, &ctx.working_dir) {
            return ToolResult::fail(msg);
        }

        if !path.exists() {
            return ToolResult::fail(format!("File not found: {file_path}"));
        }

        // Block editing sensitive files.
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
                    let positions =
                        edit_replacers::find_occurrence_positions(&content, &actual_old);
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
            let formatted =
                formatter::format_file(path.to_str().unwrap_or(file_path), &ctx.working_dir);

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

            (output_text, metadata)
        }; // lock guard dropped here

        // Collect LSP diagnostics after multi-edit (requires no lock held)
        let mut output_text = output_text;
        if let Some(diag_output) =
            diagnostics_helper::collect_post_edit_diagnostics(ctx, &path).await
        {
            output_text.push_str(&diag_output);
        }

        ToolResult::ok_with_metadata(output_text, metadata)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "multi_edit_tests.rs"]
mod tests;
