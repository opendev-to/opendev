//! Search tools — `grep` (ripgrep) and `ast_grep` (ast-grep structural search).

mod ast_grep_tool;
mod backends;
mod excludes;
mod grep_tool;
mod types;

use std::time::Duration;

pub use ast_grep_tool::AstGrepTool;
pub use excludes::{DEFAULT_SEARCH_EXCLUDE_GLOBS, DEFAULT_SEARCH_EXCLUDES, default_ignore_file};
pub use grep_tool::GrepTool;

// ===========================================================================
// Shared constants and helpers
// ===========================================================================

const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Apply offset and head_limit to output lines.
fn apply_pagination(output: &str, offset: usize, head_limit: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let start = offset.min(lines.len());
    let selected = &lines[start..];
    let selected = if head_limit > 0 {
        &selected[..head_limit.min(selected.len())]
    } else {
        selected
    };
    let mut result = selected.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    result
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests;
