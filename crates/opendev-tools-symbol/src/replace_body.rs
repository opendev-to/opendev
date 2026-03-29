//! Replace symbol body while optionally preserving the signature.
//!
//! Reads the file, finds the symbol via LSP, determines where the body starts
//! (language-aware), and replaces the body content.

use std::path::{Path, PathBuf};

use opendev_tools_lsp::protocol::UnifiedSymbolInfo;
use serde_json::Value;

use crate::error::{SymbolError, ToolResult};
use crate::util::LangCategory;

/// Handle the `replace_symbol_body` tool invocation.
///
/// Arguments:
/// - `symbol_name` (required): Name of the symbol whose body to replace.
/// - `file_path` (required): File containing the symbol.
/// - `new_body` (required): The new body content.
/// - `preserve_signature` (optional, default true): Keep the function/class signature.
pub fn handle_replace_symbol_body(arguments: &Value, workspace_root: &Path) -> ToolResult {
    let symbol_name = match arguments.get("symbol_name").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return ToolResult::err("Missing required argument: symbol_name"),
    };

    let file_path = match arguments.get("file_path").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => PathBuf::from(s),
        _ => return ToolResult::err("Missing required argument: file_path"),
    };

    let new_body = match arguments.get("new_body").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return ToolResult::err("Missing required argument: new_body"),
    };

    let preserve_signature = arguments
        .get("preserve_signature")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if !file_path.exists() {
        return ToolResult::err(format!("File not found: {}", file_path.display()));
    }

    match find_and_replace_body(
        symbol_name,
        &file_path,
        new_body,
        preserve_signature,
        workspace_root,
    ) {
        Ok(info) => ToolResult::ok_with(
            format!(
                "Replaced body of {} `{}` in {}",
                info.kind.display_name(),
                info.name,
                file_path.display()
            ),
            serde_json::json!({
                "file_path": file_path.display().to_string(),
                "symbol_name": info.name,
                "symbol_kind": info.kind.display_name(),
            }),
        ),
        Err(e) => ToolResult::err(e.to_string()),
    }
}

/// Find a symbol and replace its body. Placeholder for LSP integration.
fn find_and_replace_body(
    symbol_name: &str,
    file_path: &Path,
    _new_body: &str,
    _preserve_signature: bool,
    _workspace_root: &Path,
) -> Result<UnifiedSymbolInfo, SymbolError> {
    // In production, this would:
    // 1. Find the symbol via LSP documentSymbol
    // 2. Read the file
    // 3. Determine body start (language-aware)
    // 4. Replace the body range
    // 5. Write back
    //
    // For now, demonstrate the replace_range logic with a dummy symbol.
    // The actual LSP integration returns a UnifiedSymbolInfo.
    Err(SymbolError::SymbolNotFound(format!(
        "'{}' in {}",
        symbol_name,
        file_path.display()
    )))
}

/// Find where the body starts, skipping the signature.
///
/// For Python: find the colon after `def`/`class`, then skip to first code line.
/// For C-like: find the opening brace `{`.
pub fn find_body_start(
    lines: &[&str],
    start_line: usize,
    start_char: usize,
    lang: LangCategory,
) -> Option<(usize, usize)> {
    match lang {
        LangCategory::Python => find_body_start_python(lines, start_line, start_char),
        LangCategory::CLike => find_body_start_c_like(lines, start_line, start_char),
        LangCategory::Other => None,
    }
}

/// Find body start for Python functions/classes (after the colon).
fn find_body_start_python(
    lines: &[&str],
    start_line: usize,
    _start_char: usize,
) -> Option<(usize, usize)> {
    // Find the colon that ends the signature
    for i in start_line..lines.len() {
        let line = lines[i];
        if let Some(colon_pos) = line.find(':') {
            let after_colon = &line[colon_pos + 1..].trim();

            // Check if code follows the colon on the same line
            if !after_colon.is_empty() && !after_colon.starts_with('#') {
                return Some((
                    i,
                    colon_pos
                        + 1
                        + (line.len()
                            - line[colon_pos + 1..].len()
                            - line[colon_pos + 1..].trim_start().len()),
                ));
            }

            // Otherwise, move to the next non-empty line
            let mut body_line = i + 1;
            while body_line < lines.len() {
                let trimmed = lines[body_line].trim();
                if trimmed.is_empty() {
                    body_line += 1;
                    continue;
                }
                // Skip docstrings
                if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
                    // Find end of docstring
                    let quote = &trimmed[..3];
                    if trimmed.len() > 3 && trimmed[3..].contains(quote) {
                        // Single-line docstring
                        body_line += 1;
                        continue;
                    }
                    // Multi-line docstring
                    body_line += 1;
                    while body_line < lines.len() {
                        if lines[body_line].contains(quote) {
                            body_line += 1;
                            break;
                        }
                        body_line += 1;
                    }
                    continue;
                }
                // Found the first actual code line
                let indent = lines[body_line].len() - lines[body_line].trim_start().len();
                return Some((body_line, indent));
            }

            return None;
        }
    }
    None
}

/// Find body start for C-like languages (after the opening brace).
fn find_body_start_c_like(
    lines: &[&str],
    start_line: usize,
    _start_char: usize,
) -> Option<(usize, usize)> {
    for i in start_line..lines.len() {
        if let Some(brace_pos) = lines[i].find('{') {
            // Body starts after the brace
            let after = &lines[i][brace_pos + 1..].trim();
            if !after.is_empty() {
                return Some((i, brace_pos + 1));
            }
            // Next line
            if i + 1 < lines.len() {
                let indent = lines[i + 1].len() - lines[i + 1].trim_start().len();
                return Some((i + 1, indent));
            }
            return Some((i, brace_pos + 1));
        }
    }
    None
}

/// Replace a range in a lines buffer with new content.
///
/// If `preserve_signature` is true, keeps everything before `(start_line, start_char)`
/// and appends the new body.
pub fn replace_range(
    lines: &[&str],
    start_line: usize,
    start_char: usize,
    end_line: usize,
    end_char: usize,
    new_body: &str,
    preserve_signature: bool,
) -> String {
    let mut result = String::new();

    // Lines before the symbol
    for line in &lines[..start_line] {
        result.push_str(line);
        result.push('\n');
    }

    if preserve_signature && start_line < lines.len() {
        // Keep the signature portion
        let sig_line = lines[start_line];
        let safe_start = start_char.min(sig_line.len());
        result.push_str(&sig_line[..safe_start]);

        // Ensure newline between signature and body
        if !result.ends_with('\n') && !new_body.starts_with('\n') {
            result.push('\n');
        }
    }

    // Add new body
    result.push_str(new_body);

    // Ensure body ends with newline
    if !new_body.ends_with('\n') {
        result.push('\n');
    }

    // Remaining content after the symbol
    if end_line < lines.len() {
        let end_line_content = lines[end_line];
        let safe_end = end_char.min(end_line_content.len());
        let remainder = &end_line_content[safe_end..];
        if !remainder.is_empty() {
            result.push_str(remainder);
            result.push('\n');
        }
    }

    // Lines after the symbol
    for line in lines.iter().skip(end_line + 1) {
        result.push_str(line);
        result.push('\n');
    }

    result
}

#[cfg(test)]
#[path = "replace_body_tests.rs"]
mod tests;
