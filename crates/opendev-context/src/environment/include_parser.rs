//! @include directive parser for instruction files.
//!
//! Supports `@path`, `@./relative`, `@~/home`, `@/absolute` syntax inside
//! instruction files to compose instructions from multiple sources.
//! Maximum include depth is 5 levels with circular reference prevention.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tracing::debug;

use super::{InstructionFile, InstructionSource};

/// Maximum depth for recursive @include directives.
const MAX_INCLUDE_DEPTH: usize = 5;

/// Maximum content size per included file (50 KB).
const MAX_INCLUDE_BYTES: usize = 50 * 1024;

/// File extensions allowed for @include directives.
const INCLUDABLE_EXTENSIONS: &[&str] = &[
    "md",
    "txt",
    "rs",
    "py",
    "js",
    "ts",
    "tsx",
    "jsx",
    "toml",
    "yaml",
    "yml",
    "json",
    "sh",
    "bash",
    "zsh",
    "go",
    "c",
    "h",
    "cpp",
    "hpp",
    "java",
    "kt",
    "rb",
    "sql",
    "swift",
    "css",
    "html",
    "xml",
    "proto",
    "graphql",
    "cfg",
    "ini",
    "conf",
    "env",
    "makefile",
    "dockerfile",
    "cmake",
];

/// Process @include directives in instruction content.
///
/// Extracts `@path` directives from non-code-block lines and recursively
/// loads the referenced files. Returns the cleaned content (with @directives
/// removed) and a list of included instruction files.
///
/// Included files appear BEFORE the including file for correct priority ordering
/// (later content has higher effective priority with LLMs).
pub fn process_includes(
    content: &str,
    base_dir: &Path,
    working_dir: &Path,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
    parent_path: Option<&Path>,
) -> (String, Vec<InstructionFile>) {
    if depth >= MAX_INCLUDE_DEPTH {
        debug!(
            depth,
            "Max @include depth reached, skipping further includes"
        );
        return (content.to_string(), Vec::new());
    }

    let mut included_files = Vec::new();
    let mut cleaned_lines = Vec::new();
    let mut in_code_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Track fenced code blocks
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            cleaned_lines.push(line.to_string());
            continue;
        }

        // Only process @directives outside code blocks
        if !in_code_block
            && trimmed.starts_with('@')
            && !trimmed.starts_with("@@")
            && let Some(included) =
                resolve_include(trimmed, base_dir, working_dir, depth, visited, parent_path)
        {
            included_files.extend(included);
            // Replace the @directive line with an empty line to preserve line numbers
            cleaned_lines.push(String::new());
            continue;
        }

        cleaned_lines.push(line.to_string());
    }

    let cleaned_content = cleaned_lines.join("\n");
    (cleaned_content, included_files)
}

/// Resolve a single @include directive into instruction files.
fn resolve_include(
    directive: &str,
    base_dir: &Path,
    working_dir: &Path,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
    parent_path: Option<&Path>,
) -> Option<Vec<InstructionFile>> {
    // Extract the path from `@path` or `@path#fragment`
    let raw_path = directive.trim_start_matches('@');
    if raw_path.is_empty() {
        return None;
    }

    // Strip fragment identifier (e.g., #heading)
    let raw_path = raw_path.split('#').next().unwrap_or(raw_path);
    if raw_path.is_empty() {
        return None;
    }

    // Resolve the path
    let resolved = if raw_path.starts_with('/') {
        PathBuf::from(raw_path)
    } else if let Some(rest) = raw_path.strip_prefix("~/") {
        dirs_next::home_dir()?.join(rest)
    } else {
        // Relative to the including file's directory (or base_dir)
        let rel = raw_path.strip_prefix("./").unwrap_or(raw_path);
        base_dir.join(rel)
    };

    // Canonicalize and check for circular references
    let canonical = resolved.canonicalize().ok()?;

    if !canonical.is_file() {
        return None;
    }

    // Check extension allowlist
    if let Some(ext) = canonical.extension().and_then(|e| e.to_str()) {
        if !INCLUDABLE_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
            debug!(path = %canonical.display(), ext, "Skipping @include: non-text file extension");
            return None;
        }
    } else {
        // Special case: extensionless files with known names
        let name = canonical.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let name_lower = name.to_lowercase();
        if !["makefile", "dockerfile"].contains(&name_lower.as_str()) {
            debug!(path = %canonical.display(), "Skipping @include: no file extension");
            return None;
        }
    }

    if !visited.insert(canonical.clone()) {
        debug!(path = %canonical.display(), "Skipping @include: circular reference");
        return None;
    }

    // Read and truncate
    let content = std::fs::read_to_string(&canonical).ok()?;
    if content.trim().is_empty() {
        return Some(Vec::new());
    }

    let content = if content.len() > MAX_INCLUDE_BYTES {
        let truncated = &content[..MAX_INCLUDE_BYTES];
        format!(
            "{truncated}\n\n... (truncated, included file is {} KB)",
            content.len() / 1024
        )
    } else {
        content
    };

    // Recursively process includes in the included file
    let include_dir = canonical.parent().unwrap_or(base_dir);
    let (processed_content, nested_includes) = process_includes(
        &content,
        include_dir,
        working_dir,
        depth + 1,
        visited,
        Some(&canonical),
    );

    let mut result = nested_includes;
    result.push(InstructionFile {
        scope: "include".to_string(),
        path: canonical,
        content: processed_content,
        source: InstructionSource::Include,
        path_globs: None,
        included_from: parent_path.map(|p| p.to_path_buf()),
    });

    Some(result)
}

#[cfg(test)]
#[path = "include_parser_tests.rs"]
mod tests;
