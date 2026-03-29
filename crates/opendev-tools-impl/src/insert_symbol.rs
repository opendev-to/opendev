//! Insert content before or after a symbol found via AST-based symbol navigation.
//!
//! Uses `opendev-tools-symbol` to locate a symbol's position in a file, then
//! inserts user-provided content immediately before or after the symbol's range.

use std::collections::HashMap;
use std::path::Path;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::path_utils::{resolve_file_path, validate_path_access};

/// Shared insertion logic used by both tools.
///
/// `position` is either `"before"` or `"after"`, controlling where the content
/// is placed relative to the symbol.
fn execute_insert(
    args: &HashMap<String, serde_json::Value>,
    ctx: &ToolContext,
    position: InsertPosition,
) -> ToolResult {
    let file_path_str = match args.get("file_path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p,
        _ => return ToolResult::fail("file_path is required"),
    };

    let symbol_name = match args.get("symbol_name").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return ToolResult::fail("symbol_name is required"),
    };

    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) if !c.is_empty() => c,
        _ => return ToolResult::fail("content is required"),
    };

    let path = resolve_file_path(file_path_str, &ctx.working_dir);

    if let Err(msg) = validate_path_access(&path, &ctx.working_dir) {
        return ToolResult::fail(msg);
    }

    if !path.exists() {
        return ToolResult::fail(format!("File not found: {file_path_str}"));
    }

    let file_content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return ToolResult::fail(format!("Failed to read file: {e}")),
    };

    // Find the symbol by scanning for a line that defines it.
    // This uses a simple heuristic: look for common definition patterns
    // (fn, struct, enum, class, def, const, let, pub, impl, trait, type, interface, etc.)
    // that contain the symbol name.
    let lines: Vec<&str> = file_content.lines().collect();

    let symbol_range = match find_symbol_range(&lines, symbol_name) {
        Some(range) => range,
        None => {
            return ToolResult::fail(format!(
                "Symbol '{}' not found in {}",
                symbol_name, file_path_str
            ));
        }
    };

    // Build the new file content with insertion
    let new_content = insert_content(&file_content, &lines, content, &symbol_range, position);

    // Write back atomically
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp_path = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));

    if let Err(e) = std::fs::write(&tmp_path, &new_content) {
        return ToolResult::fail(format!("Failed to write temp file: {e}"));
    }
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        let _ = std::fs::remove_file(&tmp_path);
        return ToolResult::fail(format!("Failed to rename temp file: {e}"));
    }

    let label = match position {
        InsertPosition::Before => "before",
        InsertPosition::After => "after",
    };

    ToolResult::ok(format!(
        "Inserted content {label} symbol '{}' in {}",
        symbol_name, file_path_str
    ))
}

#[derive(Debug, Clone, Copy)]
enum InsertPosition {
    Before,
    After,
}

/// Range of a symbol in a file (0-indexed line numbers, inclusive).
#[derive(Debug)]
struct SymbolRange {
    /// First line of the symbol definition.
    start_line: usize,
    /// Last line of the symbol definition (inclusive).
    end_line: usize,
}

/// Find a symbol's line range using pattern matching on definition keywords.
///
/// Supports common patterns across languages: `fn`, `pub fn`, `struct`, `enum`,
/// `trait`, `impl`, `type`, `const`, `static`, `let`, `class`, `def`, `interface`,
/// `function`, `var`, `export`.
fn find_symbol_range(lines: &[&str], symbol_name: &str) -> Option<SymbolRange> {
    // Definition keywords that typically precede a symbol name.
    let keywords = [
        "fn ",
        "pub fn ",
        "pub(crate) fn ",
        "pub(super) fn ",
        "struct ",
        "pub struct ",
        "pub(crate) struct ",
        "enum ",
        "pub enum ",
        "pub(crate) enum ",
        "trait ",
        "pub trait ",
        "pub(crate) trait ",
        "impl ",
        "pub impl ",
        "type ",
        "pub type ",
        "pub(crate) type ",
        "const ",
        "pub const ",
        "pub(crate) const ",
        "static ",
        "pub static ",
        "pub(crate) static ",
        "let ",
        "let mut ",
        "class ",
        "def ",
        "interface ",
        "function ",
        "var ",
        "export ",
        "async fn ",
        "pub async fn ",
        "pub(crate) async fn ",
        "unsafe fn ",
        "pub unsafe fn ",
        "macro_rules! ",
    ];

    let mut start_line = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Check if this line contains a symbol definition matching the name.
        let is_definition = keywords.iter().any(|kw| {
            if let Some(rest) = trimmed.strip_prefix(kw) {
                // The symbol name should appear at the start of what follows the keyword.
                // Handle cases like `fn foo(`, `fn foo {`, `fn foo:`, `fn foo<`, `fn foo `
                let name_part = rest
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("");
                name_part == symbol_name
            } else {
                false
            }
        });

        if is_definition {
            start_line = Some(i);
            break;
        }
    }

    let start = start_line?;

    // Find the end of the symbol by tracking brace/indent depth.
    let end = find_symbol_end(lines, start);

    Some(SymbolRange {
        start_line: start,
        end_line: end,
    })
}

/// Find the end line of a symbol starting at `start_line`.
///
/// Uses brace matching for C-like languages, or indentation for Python-style.
fn find_symbol_end(lines: &[&str], start_line: usize) -> usize {
    let start_trimmed = lines[start_line].trim();

    // Check if this looks like a Python-style definition (ends with colon or has `def`/`class`)
    let is_python_style = start_trimmed.starts_with("def ") || start_trimmed.starts_with("class ");

    if is_python_style {
        return find_symbol_end_by_indent(lines, start_line);
    }

    // For C-like: track brace depth
    let mut depth: i32 = 0;
    let mut found_open_brace = false;

    for (i, line) in lines.iter().enumerate().skip(start_line) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                found_open_brace = true;
            } else if ch == '}' {
                depth -= 1;
                if found_open_brace && depth == 0 {
                    return i;
                }
            }
        }
        // If line ends with `;` and we haven't seen a brace, it's a single-line definition
        if !found_open_brace && line.trim().ends_with(';') {
            return i;
        }
    }

    // Fallback: return start line if we can't determine the end
    start_line
}

/// Find symbol end by indentation (Python-style).
fn find_symbol_end_by_indent(lines: &[&str], start_line: usize) -> usize {
    let start_indent = lines[start_line].len() - lines[start_line].trim_start().len();
    let mut last_body_line = start_line;

    for (i, line) in lines.iter().enumerate().skip(start_line + 1) {
        if line.trim().is_empty() {
            continue; // Skip blank lines
        }
        let indent = line.len() - line.trim_start().len();
        if indent <= start_indent {
            break; // Back to same or lower indent level
        }
        last_body_line = i;
    }

    last_body_line
}

/// Insert content before or after the symbol range.
fn insert_content(
    original: &str,
    lines: &[&str],
    content: &str,
    range: &SymbolRange,
    position: InsertPosition,
) -> String {
    let mut result = String::with_capacity(original.len() + content.len() + 2);

    match position {
        InsertPosition::Before => {
            // Add all lines before the symbol
            for line in &lines[..range.start_line] {
                result.push_str(line);
                result.push('\n');
            }
            // Add the inserted content
            result.push_str(content);
            if !content.ends_with('\n') {
                result.push('\n');
            }
            // Add the symbol and everything after
            for line in &lines[range.start_line..] {
                result.push_str(line);
                result.push('\n');
            }
        }
        InsertPosition::After => {
            // Add all lines up to and including the symbol
            for line in &lines[..=range.end_line] {
                result.push_str(line);
                result.push('\n');
            }
            // Add the inserted content
            result.push_str(content);
            if !content.ends_with('\n') {
                result.push('\n');
            }
            // Add remaining lines after the symbol
            if range.end_line + 1 < lines.len() {
                for line in &lines[range.end_line + 1..] {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        }
    }

    // Preserve original trailing newline behavior
    if !original.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

// ---------------------------------------------------------------------------
// InsertBeforeSymbolTool
// ---------------------------------------------------------------------------

/// Tool for inserting content before a symbol in a file.
#[derive(Debug)]
pub struct InsertBeforeSymbolTool;

#[async_trait::async_trait]
impl BaseTool for InsertBeforeSymbolTool {
    fn name(&self) -> &str {
        "insert_before_symbol"
    }

    fn description(&self) -> &str {
        "Insert content before a symbol (function, class, struct, etc.) in a file. \
         The symbol is located by name using pattern matching on common definition keywords."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file containing the symbol"
                },
                "symbol_name": {
                    "type": "string",
                    "description": "Name of the symbol to insert before (e.g. function name, struct name)"
                },
                "content": {
                    "type": "string",
                    "description": "The text content to insert before the symbol"
                }
            },
            "required": ["file_path", "symbol_name", "content"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        execute_insert(&args, ctx, InsertPosition::Before)
    }
}

// ---------------------------------------------------------------------------
// InsertAfterSymbolTool
// ---------------------------------------------------------------------------

/// Tool for inserting content after a symbol in a file.
#[derive(Debug)]
pub struct InsertAfterSymbolTool;

#[async_trait::async_trait]
impl BaseTool for InsertAfterSymbolTool {
    fn name(&self) -> &str {
        "insert_after_symbol"
    }

    fn description(&self) -> &str {
        "Insert content after a symbol (function, class, struct, etc.) in a file. \
         The symbol is located by name using pattern matching on common definition keywords."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file containing the symbol"
                },
                "symbol_name": {
                    "type": "string",
                    "description": "Name of the symbol to insert after (e.g. function name, struct name)"
                },
                "content": {
                    "type": "string",
                    "description": "The text content to insert after the symbol"
                }
            },
            "required": ["file_path", "symbol_name", "content"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        execute_insert(&args, ctx, InsertPosition::After)
    }
}

#[cfg(test)]
#[path = "insert_symbol_tests.rs"]
mod tests;

