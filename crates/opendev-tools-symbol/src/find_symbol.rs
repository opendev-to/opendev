//! Find symbol by name pattern.
//!
//! Searches for symbols matching a pattern in the workspace or a specific file,
//! returning their locations, kinds, and body previews.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tracing::warn;

use opendev_tools_lsp::protocol::{SourceRange, UnifiedSymbolInfo};

use crate::error::{SymbolError, ToolResult};
use crate::util::{relative_display, truncate};

/// Maximum characters for a body preview.
const MAX_PREVIEW_CHARS: usize = 200;
/// Maximum lines for a body preview.
const MAX_PREVIEW_LINES: usize = 5;

/// Handle the `find_symbol` tool invocation.
///
/// Arguments:
/// - `symbol_name` (required): Pattern to search for.
/// - `file_path` (optional): Restrict search to a specific file.
pub fn handle_find_symbol(arguments: &Value, workspace_root: &Path) -> ToolResult {
    let symbol_name = match arguments.get("symbol_name").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return ToolResult::err("Missing required argument: symbol_name"),
    };

    let file_path = arguments
        .get("file_path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);

    // In a full implementation, this would call the LSP SymbolRetriever.
    // For now, we provide the handler structure that formats results.
    // The actual LSP integration is done by the caller passing symbol results.
    match find_symbols_in_workspace(symbol_name, file_path.as_deref(), workspace_root) {
        Ok(symbols) => format_symbol_results(&symbols, workspace_root),
        Err(e) => ToolResult::err(e.to_string()),
    }
}

/// Search for symbols matching the given pattern.
///
/// This is a placeholder for LSP integration. In production, this delegates to
/// `LspHandler::send_request("textDocument/documentSymbol", ...)` or
/// `workspace/symbol` depending on whether a file path is provided.
fn find_symbols_in_workspace(
    _symbol_name: &str,
    _file_path: Option<&Path>,
    _workspace_root: &Path,
) -> Result<Vec<UnifiedSymbolInfo>, SymbolError> {
    // LSP integration point — returns empty for now.
    // Real implementation would:
    // 1. If file_path given, send documentSymbol request and filter by name
    // 2. Otherwise, send workspace/symbol request with query = symbol_name
    Ok(Vec::new())
}

/// Format found symbols into a tool result with output text and structured data.
pub fn format_symbol_results(symbols: &[UnifiedSymbolInfo], workspace_root: &Path) -> ToolResult {
    if symbols.is_empty() {
        return ToolResult::ok("No symbols found matching the pattern.");
    }

    let mut output = String::new();
    let mut symbol_dicts = Vec::new();

    for sym in symbols {
        let rel_path = relative_display(&sym.file_path, workspace_root);
        let line_num = sym.range.start.line + 1; // 1-indexed for display

        let _ = writeln!(
            output,
            "{} `{}` — {}:{}",
            sym.kind.display_name(),
            sym.name,
            rel_path,
            line_num,
        );

        // Try to get a body preview
        if let Some(preview) = get_body_preview(&sym.file_path, &sym.range) {
            let _ = writeln!(output, "  {}", preview);
        }

        symbol_dicts.push(serde_json::json!({
            "name": sym.name,
            "kind": sym.kind.display_name(),
            "file_path": sym.file_path.display().to_string(),
            "line": line_num,
            "container": sym.container_name,
        }));
    }

    ToolResult::ok_with(output, serde_json::json!({ "symbols": symbol_dicts }))
}

/// Extract a body preview from the source file at the given range.
fn get_body_preview(file_path: &Path, range: &SourceRange) -> Option<String> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read file for preview: {}", e);
            return None;
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    let start = range.start.line as usize;
    let end = (range.end.line as usize).min(lines.len().saturating_sub(1));

    if start >= lines.len() {
        return None;
    }

    let preview_end = end.min(start + MAX_PREVIEW_LINES - 1);
    let preview_lines: Vec<&str> = lines[start..=preview_end].to_vec();
    let mut preview = preview_lines.join("\n");

    preview = truncate(&preview, MAX_PREVIEW_CHARS);

    Some(preview)
}

#[cfg(test)]
#[path = "find_symbol_tests.rs"]
mod tests;
