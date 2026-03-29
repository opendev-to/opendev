//! Find all references to a symbol.
//!
//! Groups references by file and formats output with line previews.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tracing::warn;

use crate::error::{SymbolError, ToolResult};
use crate::util::{relative_display, truncate};

/// A reference location in source code.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SymbolReference {
    pub file: PathBuf,
    pub line: u32,
    pub character: u32,
}

/// Handle the `find_referencing_symbols` tool invocation.
///
/// Arguments:
/// - `symbol_name` (required): The symbol to find references for.
/// - `file_path` (required): The file containing the symbol definition.
/// - `include_declaration` (optional, default true): Whether to include the declaration.
pub fn handle_find_references(arguments: &Value, workspace_root: &Path) -> ToolResult {
    let symbol_name = match arguments.get("symbol_name").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return ToolResult::err("Missing required argument: symbol_name"),
    };

    let file_path = match arguments.get("file_path").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => PathBuf::from(s),
        _ => return ToolResult::err("Missing required argument: file_path"),
    };

    let include_declaration = arguments
        .get("include_declaration")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    match find_references(symbol_name, &file_path, include_declaration, workspace_root) {
        Ok(refs) => format_reference_results(&refs, workspace_root),
        Err(e) => ToolResult::err(e.to_string()),
    }
}

/// Find references for a symbol. Placeholder for LSP integration.
fn find_references(
    _symbol_name: &str,
    _file_path: &Path,
    _include_declaration: bool,
    _workspace_root: &Path,
) -> Result<Vec<SymbolReference>, SymbolError> {
    // LSP integration point: send textDocument/references request.
    Ok(Vec::new())
}

/// Format found references into a tool result, grouped by file.
pub fn format_reference_results(refs: &[SymbolReference], workspace_root: &Path) -> ToolResult {
    if refs.is_empty() {
        return ToolResult::ok("No references found.");
    }

    // Group by file
    let mut by_file: std::collections::BTreeMap<&Path, Vec<&SymbolReference>> =
        std::collections::BTreeMap::new();
    for r in refs {
        by_file.entry(r.file.as_path()).or_default().push(r);
    }

    let file_count = by_file.len();
    let total_count = refs.len();

    let mut output = String::new();
    let _ = writeln!(
        output,
        "Found {} reference(s) across {} file(s):",
        total_count, file_count
    );

    for (file, file_refs) in &by_file {
        let rel = relative_display(file, workspace_root);
        let _ = writeln!(output, "\n  {}:", rel);

        for r in file_refs {
            let line_preview = read_line_preview(file, r.line);
            let preview = line_preview
                .as_deref()
                .map(|s| truncate(s.trim(), 80))
                .unwrap_or_default();

            let _ = writeln!(
                output,
                "    L{}:{} {}",
                r.line + 1,
                r.character + 1,
                preview
            );
        }
    }

    let ref_dicts: Vec<Value> = refs
        .iter()
        .map(|r| {
            serde_json::json!({
                "file": r.file.display().to_string(),
                "line": r.line + 1,
                "character": r.character + 1,
            })
        })
        .collect();

    ToolResult::ok_with(
        output,
        serde_json::json!({
            "references": ref_dicts,
            "file_count": file_count,
            "total_count": total_count,
        }),
    )
}

/// Read a single line from a file for preview purposes.
fn read_line_preview(file: &Path, line: u32) -> Option<String> {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read file for preview: {}", e);
            return None;
        }
    };

    content.lines().nth(line as usize).map(|s| s.to_string())
}

#[cfg(test)]
#[path = "find_references_tests.rs"]
mod tests;
