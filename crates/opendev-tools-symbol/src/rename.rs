//! Rename symbol across files.
//!
//! Applies workspace edits in reverse order to preserve line numbers.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tracing::{debug, warn};

use opendev_tools_lsp::protocol::{TextEdit, WorkspaceEdit};

use crate::error::{SymbolError, ToolResult};
use crate::util::{is_valid_identifier, relative_display};

/// Handle the `rename_symbol` tool invocation.
///
/// Arguments:
/// - `symbol_name` (required): Current name of the symbol.
/// - `file_path` (required): File containing the symbol.
/// - `new_name` (required): The new name.
pub fn handle_rename_symbol(arguments: &Value, workspace_root: &Path) -> ToolResult {
    let symbol_name = match arguments.get("symbol_name").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return ToolResult::err("Missing required argument: symbol_name"),
    };

    let file_path = match arguments.get("file_path").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => PathBuf::from(s),
        _ => return ToolResult::err("Missing required argument: file_path"),
    };

    let new_name = match arguments.get("new_name").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return ToolResult::err("Missing required argument: new_name"),
    };

    if !is_valid_identifier(new_name) {
        return ToolResult::err(format!(
            "Invalid identifier '{}': must start with letter/underscore and contain only alphanumeric/underscore",
            new_name
        ));
    }

    if !file_path.exists() {
        return ToolResult::err(format!("File not found: {}", file_path.display()));
    }

    match perform_rename(symbol_name, &file_path, new_name, workspace_root) {
        Ok(edit) => apply_workspace_edit(&edit, workspace_root),
        Err(e) => ToolResult::err(e.to_string()),
    }
}

/// Perform the rename via LSP. Placeholder for LSP integration.
fn perform_rename(
    _symbol_name: &str,
    _file_path: &Path,
    _new_name: &str,
    _workspace_root: &Path,
) -> Result<WorkspaceEdit, SymbolError> {
    // LSP integration point: send textDocument/rename request.
    // Returns a WorkspaceEdit with changes across potentially multiple files.
    Ok(WorkspaceEdit::new())
}

/// Apply a workspace edit, writing changes to each affected file.
///
/// Edits within each file are applied in reverse order (bottom to top)
/// to preserve line/character offsets.
pub fn apply_workspace_edit(edit: &WorkspaceEdit, workspace_root: &Path) -> ToolResult {
    if edit.changes.is_empty() {
        return ToolResult::err("Rename returned no changes. Symbol may not be found.");
    }

    let mut modified_files = Vec::new();
    let mut total_changes = 0usize;

    for (file_path, edits) in &edit.changes {
        match apply_file_edits(file_path, edits) {
            Ok(count) => {
                total_changes += count;
                modified_files.push(relative_display(file_path, workspace_root));
                debug!("Applied {} edits to {}", count, file_path.display());
            }
            Err(e) => {
                warn!("Failed to apply edits to {}: {}", file_path.display(), e);
                return ToolResult::err(format!(
                    "Failed to apply edits to {}: {}",
                    file_path.display(),
                    e
                ));
            }
        }
    }

    let output = format!(
        "Renamed symbol: {} change(s) across {} file(s).\nModified: {}",
        total_changes,
        modified_files.len(),
        modified_files.join(", ")
    );

    ToolResult::ok_with(
        output,
        serde_json::json!({
            "modified_files": modified_files,
            "total_changes": total_changes,
        }),
    )
}

/// Apply edits to a single file in reverse order.
fn apply_file_edits(file_path: &Path, edits: &[TextEdit]) -> Result<usize, SymbolError> {
    let content = std::fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();

    // Sort edits in reverse order (bottom-to-top) to preserve offsets
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then(b.range.start.character.cmp(&a.range.start.character))
    });

    for edit in &sorted {
        apply_single_edit(&mut lines, edit)?;
    }

    let result = lines.join("\n");
    std::fs::write(file_path, result)?;

    Ok(sorted.len())
}

/// Apply a single text edit to a lines buffer.
fn apply_single_edit(lines: &mut Vec<String>, edit: &TextEdit) -> Result<(), SymbolError> {
    let start_line = edit.range.start.line as usize;
    let start_char = edit.range.start.character as usize;
    let end_line = edit.range.end.line as usize;
    let end_char = edit.range.end.character as usize;

    // Ensure lines are long enough
    while lines.len() <= end_line {
        lines.push(String::new());
    }

    if start_line == end_line {
        // Single-line edit
        let line = &lines[start_line];
        let safe_start = start_char.min(line.len());
        let safe_end = end_char.min(line.len());
        let new_line = format!(
            "{}{}{}",
            &line[..safe_start],
            edit.new_text,
            &line[safe_end..]
        );
        lines[start_line] = new_line;
    } else {
        // Multi-line edit
        let first = &lines[start_line];
        let last = &lines[end_line];
        let safe_start = start_char.min(first.len());
        let safe_end = end_char.min(last.len());

        let new_line = format!(
            "{}{}{}",
            &first[..safe_start],
            edit.new_text,
            &last[safe_end..]
        );

        // Remove lines from start_line to end_line and insert new_line
        lines.drain(start_line..=end_line);
        lines.insert(start_line, new_line);
    }

    Ok(())
}

#[cfg(test)]
#[path = "rename_tests.rs"]
mod tests;
