//! Memory tool — search and write memory files for cross-session persistence.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use opendev_tools_core::{BaseTool, ToolContext, ToolDisplayMeta, ToolResult};

/// Tool for managing persistent memory files.
#[derive(Debug)]
pub struct MemoryTool;

impl MemoryTool {
    /// Default memory directory under the user's home.
    fn memory_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".opendev").join("memory"))
    }

    /// Maximum file size to read (256 KB).
    const MAX_READ_SIZE: u64 = 256 * 1024;
}

#[async_trait::async_trait]
impl BaseTool for MemoryTool {
    fn name(&self) -> &str {
        "memory"
    }

    fn description(&self) -> &str {
        "Read, write, or search persistent memory files stored in ~/.opendev/memory/."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write", "search", "list"],
                    "description": "Action to perform"
                },
                "file": {
                    "type": "string",
                    "description": "Memory file name (e.g., 'patterns.md')"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write (for write action)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for search action)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::fail("action is required"),
        };

        let memory_dir = match Self::memory_dir() {
            Some(d) => d,
            None => return ToolResult::fail("Cannot determine home directory"),
        };

        match action {
            "read" => {
                let file = match args.get("file").and_then(|v| v.as_str()) {
                    Some(f) => f,
                    None => return ToolResult::fail("file is required for read"),
                };
                memory_read(&memory_dir, file)
            }
            "write" => {
                let file = match args.get("file").and_then(|v| v.as_str()) {
                    Some(f) => f,
                    None => return ToolResult::fail("file is required for write"),
                };
                let content = match args.get("content").and_then(|v| v.as_str()) {
                    Some(c) => c,
                    None => return ToolResult::fail("content is required for write"),
                };
                memory_write(&memory_dir, file, content)
            }
            "search" => {
                let query = match args.get("query").and_then(|v| v.as_str()) {
                    Some(q) => q,
                    None => return ToolResult::fail("query is required for search"),
                };
                memory_search(&memory_dir, query)
            }
            "list" => memory_list(&memory_dir),
            _ => ToolResult::fail(format!(
                "Unknown action: {action}. Available: read, write, search, list"
            )),
        }
    }

    fn display_meta(&self) -> Option<ToolDisplayMeta> {
        Some(ToolDisplayMeta {
            verb: "Memory",
            label: "memory",
            category: "Other",
            primary_arg_keys: &["action", "file", "query"],
        })
    }
}

fn memory_read(dir: &Path, file: &str) -> ToolResult {
    // Prevent path traversal
    if file.contains("..") || file.starts_with('/') {
        return ToolResult::fail("Invalid file name (no path traversal allowed)");
    }

    let path = dir.join(file);
    if !path.exists() {
        return ToolResult::fail(format!("Memory file not found: {file}"));
    }

    match std::fs::metadata(&path) {
        Ok(m) if m.len() > MemoryTool::MAX_READ_SIZE => {
            return ToolResult::fail(format!(
                "Memory file too large ({} bytes, max {})",
                m.len(),
                MemoryTool::MAX_READ_SIZE
            ));
        }
        Err(e) => return ToolResult::fail(format!("Cannot read file: {e}")),
        _ => {}
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => ToolResult::ok(content),
        Err(e) => ToolResult::fail(format!("Failed to read {file}: {e}")),
    }
}

fn memory_write(dir: &Path, file: &str, content: &str) -> ToolResult {
    if file.contains("..") || file.starts_with('/') {
        return ToolResult::fail("Invalid file name (no path traversal allowed)");
    }

    if let Err(e) = std::fs::create_dir_all(dir) {
        return ToolResult::fail(format!("Failed to create memory directory: {e}"));
    }

    let path = dir.join(file);
    match std::fs::write(&path, content) {
        Ok(_) => ToolResult::ok(format!("Written {} bytes to {file}", content.len())),
        Err(e) => ToolResult::fail(format!("Failed to write {file}: {e}")),
    }
}

fn memory_search(dir: &Path, query: &str) -> ToolResult {
    if !dir.exists() {
        return ToolResult::ok("No memory files found (directory does not exist)".to_string());
    }

    let query_lower = query.to_lowercase();
    let keywords: Vec<&str> = query_lower.split_whitespace().collect();
    if keywords.is_empty() {
        return ToolResult::fail("Search query cannot be empty");
    }

    let mut results = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => return ToolResult::fail(format!("Failed to read memory directory: {e}")),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let content_lower = content.to_lowercase();
        let score: usize = keywords
            .iter()
            .filter(|kw| content_lower.contains(*kw))
            .count();

        if score > 0 {
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Collect matching lines
            let mut matching_lines = Vec::new();
            for (i, line) in content.lines().enumerate() {
                let line_lower = line.to_lowercase();
                if keywords.iter().any(|kw| line_lower.contains(*kw)) {
                    matching_lines.push(format!("  {}:{}: {}", filename, i + 1, line));
                    if matching_lines.len() >= 5 {
                        break;
                    }
                }
            }

            results.push((score, filename, matching_lines));
        }
    }

    if results.is_empty() {
        return ToolResult::ok(format!("No matches found for '{query}'"));
    }

    // Sort by score descending
    results.sort_by(|a, b| b.0.cmp(&a.0));

    let mut output = format!("Found matches in {} files:\n\n", results.len());
    for (score, filename, lines) in &results {
        output.push_str(&format!(
            "{filename} (relevance: {score}/{}):\n",
            keywords.len()
        ));
        for line in lines {
            output.push_str(&format!("{line}\n"));
        }
        output.push('\n');
    }

    ToolResult::ok(output)
}

fn memory_list(dir: &Path) -> ToolResult {
    if !dir.exists() {
        return ToolResult::ok("No memory files (directory does not exist)".to_string());
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => return ToolResult::fail(format!("Failed to read memory directory: {e}")),
    };

    let mut files: Vec<(String, u64)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);
            files.push((name, size));
        }
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));

    if files.is_empty() {
        return ToolResult::ok("No memory files found".to_string());
    }

    let mut output = format!("Memory files ({}):\n", files.len());
    for (name, size) in &files {
        output.push_str(&format!("  {name} ({size} bytes)\n"));
    }

    ToolResult::ok(output)
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
