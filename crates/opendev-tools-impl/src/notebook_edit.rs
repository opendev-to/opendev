//! Notebook edit tool — edit Jupyter notebook (.ipynb) cells.
//!
//! Supports three edit modes:
//! - replace: Replace an existing cell's content
//! - insert: Insert a new cell at a position
//! - delete: Delete a cell
//!
//! Cells can be identified by cell_id (preferred) or cell_number (0-indexed).

use std::collections::HashMap;

use crate::path_utils::resolve_file_path;
use std::path::PathBuf;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool for editing Jupyter notebook cells.
#[derive(Debug)]
pub struct NotebookEditTool;

#[async_trait::async_trait]
impl BaseTool for NotebookEditTool {
    fn name(&self) -> &str {
        "NotebookEdit"
    }

    fn description(&self) -> &str {
        "Edit Jupyter notebook (.ipynb) cells. Supports replace, insert, and delete \
         operations. Identify cells by cell_id or cell_number (0-indexed)."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "Path to the .ipynb file"
                },
                "new_source": {
                    "type": "string",
                    "description": "New cell source content"
                },
                "cell_id": {
                    "type": "string",
                    "description": "Cell ID to edit (preferred)"
                },
                "cell_number": {
                    "type": "integer",
                    "description": "0-indexed cell position (alternative to cell_id)"
                },
                "cell_type": {
                    "type": "string",
                    "description": "Cell type: 'code' or 'markdown'",
                    "enum": ["code", "markdown"]
                },
                "edit_mode": {
                    "type": "string",
                    "description": "Operation: 'replace' (default), 'insert', or 'delete'",
                    "enum": ["replace", "insert", "delete"]
                }
            },
            "required": ["notebook_path"]
        })
    }

    fn category(&self) -> opendev_tools_core::ToolCategory {
        opendev_tools_core::ToolCategory::Write
    }

    fn should_defer(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let notebook_path = match args.get("notebook_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::fail("notebook_path is required"),
        };

        let new_source = args
            .get("new_source")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let cell_id = args.get("cell_id").and_then(|v| v.as_str());
        let cell_number = args.get("cell_number").and_then(|v| v.as_i64());
        let cell_type = args.get("cell_type").and_then(|v| v.as_str());
        let edit_mode = args
            .get("edit_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("replace");

        // Resolve path
        let path = resolve_file_path(notebook_path, &ctx.working_dir);

        // Validate
        if !path.exists() {
            return ToolResult::fail(format!("Notebook not found: {notebook_path}"));
        }

        if path.extension().and_then(|e| e.to_str()) != Some("ipynb") {
            return ToolResult::fail(format!("Not a Jupyter notebook file: {notebook_path}"));
        }

        // Load notebook
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to read notebook: {e}")),
        };

        let mut notebook: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => return ToolResult::fail(format!("Invalid notebook JSON: {e}")),
        };

        // Extract cells as owned Vec
        let cells = match notebook.get("cells").and_then(|v| v.as_array()) {
            Some(c) => c.clone(),
            None => return ToolResult::fail("Notebook has no 'cells' array"),
        };

        let result = match edit_mode {
            "replace" => replace_cell(cells, new_source, cell_id, cell_number, cell_type),
            "insert" => insert_cell(cells, new_source, cell_id, cell_number, cell_type),
            "delete" => delete_cell(cells, cell_id, cell_number),
            other => {
                return ToolResult::fail(format!(
                    "Unknown edit_mode: {other}. Use 'replace', 'insert', or 'delete'."
                ));
            }
        };

        match result {
            Ok((new_cells, tool_result)) => {
                // Save the updated notebook
                notebook["cells"] = serde_json::json!(new_cells);
                if let Err(e) = save_notebook(&path, &notebook) {
                    return ToolResult::fail(e);
                }
                tool_result
            }
            Err(tool_result) => tool_result,
        }
    }
}

/// Find a cell by ID or number, returning its index.
fn find_cell_index(
    cells: &[serde_json::Value],
    cell_id: Option<&str>,
    cell_number: Option<i64>,
) -> Result<usize, String> {
    if let Some(id) = cell_id {
        for (i, cell) in cells.iter().enumerate() {
            if cell.get("id").and_then(|v| v.as_str()) == Some(id) {
                return Ok(i);
            }
        }
        return Err(format!("Cell with ID '{id}' not found"));
    }

    if let Some(num) = cell_number {
        if num < 0 || num as usize >= cells.len() {
            return Err(format!(
                "Cell number {num} out of range (0-{})",
                cells.len().saturating_sub(1)
            ));
        }
        return Ok(num as usize);
    }

    Err("Either cell_id or cell_number must be provided".to_string())
}

/// Convert source string to notebook cell source format (list of lines).
fn source_to_lines(source: &str) -> serde_json::Value {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut result: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i < lines.len() - 1 {
            result.push(format!("{line}\n"));
        } else {
            result.push(line.to_string());
        }
    }
    serde_json::json!(result)
}

/// Save the notebook back to disk.
fn save_notebook(path: &PathBuf, notebook: &serde_json::Value) -> Result<(), String> {
    let json = serde_json::to_string_pretty(notebook)
        .map_err(|e| format!("Failed to serialize notebook: {e}"))?;
    let json = if json.ends_with('\n') {
        json
    } else {
        format!("{json}\n")
    };
    std::fs::write(path, &json).map_err(|e| format!("Failed to write notebook: {e}"))
}

/// Replace an existing cell's content. Returns (updated_cells, ToolResult) on success.
#[allow(clippy::result_large_err)]
fn replace_cell(
    mut cells: Vec<serde_json::Value>,
    new_source: &str,
    cell_id: Option<&str>,
    cell_number: Option<i64>,
    cell_type: Option<&str>,
) -> Result<(Vec<serde_json::Value>, ToolResult), ToolResult> {
    let index = find_cell_index(&cells, cell_id, cell_number).map_err(ToolResult::fail)?;

    // Get old source length for reporting
    let old_source_len = cells[index]
        .get("source")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.len())
                .sum::<usize>()
        })
        .unwrap_or(0);

    let result_cell_id = cells[index]
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Update source
    cells[index]["source"] = source_to_lines(new_source);

    // Update cell type if specified
    if let Some(ct) = cell_type {
        cells[index]["cell_type"] = serde_json::json!(ct);
    }

    let mut metadata = HashMap::new();
    metadata.insert("cell_id".into(), serde_json::json!(result_cell_id));
    metadata.insert("cell_number".into(), serde_json::json!(index));
    metadata.insert("edit_mode".into(), serde_json::json!("replace"));

    Ok((
        cells,
        ToolResult::ok_with_metadata(
            format!(
                "Replaced cell {result_cell_id} content ({old_source_len} -> {} chars)",
                new_source.len()
            ),
            metadata,
        ),
    ))
}

/// Insert a new cell. Returns (updated_cells, ToolResult) on success.
#[allow(clippy::result_large_err)]
fn insert_cell(
    mut cells: Vec<serde_json::Value>,
    new_source: &str,
    after_cell_id: Option<&str>,
    at_position: Option<i64>,
    cell_type: Option<&str>,
) -> Result<(Vec<serde_json::Value>, ToolResult), ToolResult> {
    let cell_type = cell_type.unwrap_or("code");

    // Determine insert position
    let insert_pos = if let Some(id) = after_cell_id {
        let idx = find_cell_index(&cells, Some(id), None).map_err(ToolResult::fail)?;
        idx + 1
    } else if let Some(pos) = at_position {
        let pos = pos.max(0) as usize;
        pos.min(cells.len())
    } else {
        cells.len()
    };

    // Generate a new cell ID
    let new_cell_id = format!("{:08x}", rand_u32());

    // Build new cell
    let mut new_cell = serde_json::json!({
        "id": new_cell_id,
        "cell_type": cell_type,
        "metadata": {},
        "source": source_to_lines(new_source),
    });

    if cell_type == "code" {
        new_cell["execution_count"] = serde_json::Value::Null;
        new_cell["outputs"] = serde_json::json!([]);
    }

    cells.insert(insert_pos, new_cell);

    let mut metadata = HashMap::new();
    metadata.insert("cell_id".into(), serde_json::json!(new_cell_id));
    metadata.insert("cell_number".into(), serde_json::json!(insert_pos));
    metadata.insert("edit_mode".into(), serde_json::json!("insert"));
    metadata.insert("cell_type".into(), serde_json::json!(cell_type));

    Ok((
        cells,
        ToolResult::ok_with_metadata(
            format!("Inserted new {cell_type} cell at position {insert_pos}"),
            metadata,
        ),
    ))
}

/// Delete a cell. Returns (updated_cells, ToolResult) on success.
#[allow(clippy::result_large_err)]
fn delete_cell(
    mut cells: Vec<serde_json::Value>,
    cell_id: Option<&str>,
    cell_number: Option<i64>,
) -> Result<(Vec<serde_json::Value>, ToolResult), ToolResult> {
    let index = find_cell_index(&cells, cell_id, cell_number).map_err(ToolResult::fail)?;

    let deleted = cells.remove(index);
    let deleted_cell_id = deleted
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let mut metadata = HashMap::new();
    metadata.insert("cell_id".into(), serde_json::json!(deleted_cell_id));
    metadata.insert("cell_number".into(), serde_json::json!(index));
    metadata.insert("edit_mode".into(), serde_json::json!("delete"));

    Ok((
        cells,
        ToolResult::ok_with_metadata(
            format!("Deleted cell {deleted_cell_id} (was at position {index})"),
            metadata,
        ),
    ))
}

/// Simple pseudo-random u32 (not crypto-secure, just for cell IDs).
fn rand_u32() -> u32 {
    use std::time::SystemTime;
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u32;
    let mut x = seed;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x.wrapping_mul(0x9E3779B9)
}

#[cfg(test)]
#[path = "notebook_edit_tests.rs"]
mod tests;
