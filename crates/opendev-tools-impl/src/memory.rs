//! Memory tool — search and write memory files for cross-session persistence.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use opendev_tools_core::{BaseTool, ToolContext, ToolDisplayMeta, ToolResult};

/// Tool for managing persistent memory files.
#[derive(Debug)]
pub struct MemoryTool;

impl MemoryTool {
    /// Maximum file size to read (256 KB).
    const MAX_READ_SIZE: u64 = 256 * 1024;
    /// Maximum lines in MEMORY.md index.
    const MAX_INDEX_LINES: usize = 200;
    /// Maximum bytes in MEMORY.md index.
    const MAX_INDEX_BYTES: usize = 25 * 1024;
}

/// Resolve the memory directory based on scope and working directory.
fn resolve_memory_dir(scope: &str, working_dir: &Path) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match scope {
        "global" => Some(home.join(".opendev").join("memory")),
        _ => {
            let encoded = opendev_config::paths::encode_project_path(working_dir);
            Some(
                home.join(".opendev")
                    .join("projects")
                    .join(encoded)
                    .join("memory"),
            )
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for MemoryTool {
    fn name(&self) -> &str {
        "memory"
    }

    fn description(&self) -> &str {
        "Read, write, search, or list persistent memory files. \
         Use 'scope' to target project-specific or global storage. \
         Project scope (default) stores at ~/.opendev/projects/<id>/memory/. \
         Global scope stores at ~/.opendev/memory/."
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
                },
                "scope": {
                    "type": "string",
                    "enum": ["project", "global"],
                    "description": "Memory scope: 'project' (default) or 'global'"
                },
                "mode": {
                    "type": "string",
                    "enum": ["keyword", "semantic", "auto"],
                    "description": "Search mode: 'keyword' (fast text match), 'semantic' (LLM-based), 'auto' (keyword first, semantic fallback). Default: 'auto'"
                }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> opendev_tools_core::ToolCategory {
        opendev_tools_core::ToolCategory::Memory
    }

    fn truncation_rule(&self) -> Option<opendev_tools_core::TruncationRule> {
        Some(opendev_tools_core::TruncationRule::head(10000))
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::fail("action is required"),
        };

        let scope = args
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("project");

        let memory_dir = match resolve_memory_dir(scope, &ctx.working_dir) {
            Some(d) => d,
            None => return ToolResult::fail("Cannot determine memory directory"),
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
                let result = memory_write(&memory_dir, file, content);
                if result.success {
                    let _ = update_memory_index(&memory_dir);
                }
                result
            }
            "search" => {
                let query = match args.get("query").and_then(|v| v.as_str()) {
                    Some(q) => q,
                    None => return ToolResult::fail("query is required for search"),
                };
                let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("auto");
                memory_search(&memory_dir, query, mode).await
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

/// Format a staleness note for a memory file based on its modification time.
fn format_staleness(path: &Path) -> String {
    let days_ago = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|mt| {
            std::time::SystemTime::now()
                .duration_since(mt)
                .ok()
                .map(|d| d.as_secs() / 86400)
        });

    match days_ago {
        Some(0) | None => String::new(),
        Some(d @ 1..=6) => format!("[Updated {d} day{} ago]", if d == 1 { "" } else { "s" }),
        Some(d @ 7..=30) => {
            format!("[Updated {d} days ago \u{2014} may be outdated, verify before acting]")
        }
        Some(d) => format!(
            "[WARNING: Last updated {d} days ago. Verify against current code before relying on this.]"
        ),
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
        Ok(content) => {
            let staleness = format_staleness(&path);
            let output = if staleness.is_empty() {
                content
            } else {
                format!("{staleness}\n\n{content}")
            };
            ToolResult::ok(output)
        }
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
        Ok(_) => {
            let has_type = content.contains("type:");
            let mut msg = format!("Written {} bytes to {file}", content.len());
            if !has_type {
                msg.push_str(
                    "\n\nNote: Memory file is missing a `type:` field in YAML frontmatter. \
                     Please include frontmatter with `type` (user/feedback/project/reference) \
                     and `description` fields for better retrieval.",
                );
            }
            ToolResult::ok(msg)
        }
        Err(e) => ToolResult::fail(format!("Failed to write {file}: {e}")),
    }
}

/// Compute a weighted relevance score for a memory file.
///
/// Frontmatter fields (description, type) are weighted 3x higher than body.
/// Filename matches are weighted 2x. Uses OR matching with percentage scoring.
fn compute_relevance(filename: &str, content: &str, keywords: &[&str]) -> f64 {
    if keywords.is_empty() {
        return 0.0;
    }

    let filename_lower = filename.to_lowercase();
    let content_lower = content.to_lowercase();

    // Split content into frontmatter and body
    let (frontmatter, body) = if let Some(rest) = content_lower.strip_prefix("---") {
        if let Some(end) = rest.find("---") {
            let fm = &rest[..end];
            let body = &rest[end + 3..];
            (fm.to_string(), body.to_string())
        } else {
            (String::new(), content_lower.clone())
        }
    } else {
        (String::new(), content_lower.clone())
    };

    let mut total_score = 0.0;
    for kw in keywords {
        let mut kw_score = 0.0;

        // Filename match (weight: 2x)
        if filename_lower.contains(kw) {
            kw_score += 2.0;
        }
        // Frontmatter match (weight: 3x)
        if frontmatter.contains(kw) {
            kw_score += 3.0;
        }
        // Body match (weight: 1x)
        if body.contains(kw) {
            kw_score += 1.0;
        }
        // Substring similarity bonus for partial matches
        if kw.len() >= 3 {
            for word in body.split_whitespace() {
                if word.starts_with(kw) || kw.starts_with(word) {
                    kw_score += 0.5;
                    break;
                }
            }
        }

        total_score += kw_score;
    }

    total_score
}

/// Run keyword-based search across memory files, returning scored results.
fn keyword_search(dir: &Path, query: &str) -> Vec<(f64, String, Vec<String>)> {
    let query_lower = query.to_lowercase();
    let keywords: Vec<&str> = query_lower.split_whitespace().collect();
    if keywords.is_empty() {
        return Vec::new();
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let score = compute_relevance(&filename, &content, &keywords);

        if score > 0.0 {
            let mut matching_lines = Vec::new();
            for (i, line) in content.lines().enumerate() {
                let line_lower = line.to_lowercase();
                if keywords.iter().any(|kw| line_lower.contains(kw)) {
                    matching_lines.push(format!("  {}:{}: {}", filename, i + 1, line));
                    if matching_lines.len() >= 5 {
                        break;
                    }
                }
            }
            results.push((score, filename, matching_lines));
        }
    }

    results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    results
}

/// Run LLM-based semantic search using the shared `MemorySelector`.
async fn semantic_search(dir: &Path, query: &str) -> Vec<(f64, String, Vec<String>)> {
    use opendev_agents::attachments::collectors::memory_selector::MemorySelector;

    let selector = match MemorySelector::try_new() {
        Some(s) => s,
        None => return Vec::new(),
    };

    // Build manifest of all files
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut file_info: Vec<(String, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.ends_with(".md") && n != "MEMORY.md" => n.to_string(),
            _ => continue,
        };
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let desc = extract_description(&content);
        let desc_str = if desc.is_empty() {
            "(no description)".to_string()
        } else {
            desc
        };
        file_info.push((name, desc_str));
    }

    if file_info.is_empty() {
        return Vec::new();
    }

    let manifest: String = file_info
        .iter()
        .map(|(name, desc)| format!("- {name}: {desc}"))
        .collect::<Vec<_>>()
        .join("\n");

    let filenames = match selector.select(&manifest, query).await {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    // Read selected files and return with synthetic score
    let mut results = Vec::new();
    for (rank, filename) in filenames.into_iter().take(5).enumerate() {
        let path = dir.join(&filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let first_lines: Vec<String> = content
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with("---"))
                .take(5)
                .enumerate()
                .map(|(i, l)| format!("  {}:{}: {}", filename, i + 1, l))
                .collect();
            // Higher score for higher-ranked results
            let score = 10.0 - rank as f64;
            results.push((score, filename, first_lines));
        }
    }
    results
}

fn format_search_results_with_prefix(
    results: &[(f64, String, Vec<String>)],
    query: &str,
    prefix: &str,
) -> ToolResult {
    if results.is_empty() {
        return ToolResult::ok(format!("No matches found for '{query}'"));
    }

    let mut output = format!("{prefix}Found matches in {} files:\n\n", results.len());
    for (score, filename, lines) in results {
        output.push_str(&format!("{filename} (relevance: {score:.1}):\n"));
        for line in lines {
            output.push_str(&format!("{line}\n"));
        }
        output.push('\n');
    }
    ToolResult::ok(output)
}

fn format_search_results(results: &[(f64, String, Vec<String>)], query: &str) -> ToolResult {
    if results.is_empty() {
        return ToolResult::ok(format!("No matches found for '{query}'"));
    }

    let mut output = format!("Found matches in {} files:\n\n", results.len());
    for (score, filename, lines) in results {
        output.push_str(&format!("{filename} (relevance: {score:.1}):\n"));
        for line in lines {
            output.push_str(&format!("{line}\n"));
        }
        output.push('\n');
    }
    ToolResult::ok(output)
}

async fn memory_search(dir: &Path, query: &str, mode: &str) -> ToolResult {
    if !dir.exists() {
        return ToolResult::ok("No memory files found (directory does not exist)".to_string());
    }

    if query.split_whitespace().next().is_none() {
        return ToolResult::fail("Search query cannot be empty");
    }

    match mode {
        "keyword" => {
            let results = keyword_search(dir, query);
            format_search_results(&results, query)
        }
        "semantic" => {
            let results = semantic_search(dir, query).await;
            format_search_results(&results, query)
        }
        _ => {
            // "auto": keyword first, semantic fallback if 0 results
            let results = keyword_search(dir, query);
            if !results.is_empty() {
                return format_search_results(&results, query);
            }
            let results = semantic_search(dir, query).await;
            if results.is_empty() {
                ToolResult::ok(format!("No matches found for '{query}'"))
            } else {
                format_search_results_with_prefix(&results, query, "(via semantic search) ")
            }
        }
    }
}

fn memory_list(dir: &Path) -> ToolResult {
    if !dir.exists() {
        return ToolResult::ok("No memory files (directory does not exist)".to_string());
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => return ToolResult::fail(format!("Failed to read memory directory: {e}")),
    };

    let mut files: Vec<(String, u64, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let meta = path.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let age = meta
                .and_then(|m| m.modified().ok())
                .and_then(|mt| {
                    std::time::SystemTime::now()
                        .duration_since(mt)
                        .ok()
                        .map(|d| d.as_secs() / 86400)
                })
                .map(|days| match days {
                    0 => "today".to_string(),
                    1 => "1 day ago".to_string(),
                    d => format!("{d} days ago"),
                })
                .unwrap_or_else(|| "unknown".to_string());
            files.push((name, size, age));
        }
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));

    if files.is_empty() {
        return ToolResult::ok("No memory files found".to_string());
    }

    let mut output = format!("Memory files ({}):\n", files.len());
    for (name, size, age) in &files {
        output.push_str(&format!("  {name} ({size} bytes, {age})\n"));
    }

    ToolResult::ok(output)
}

/// Regenerate the MEMORY.md index from all `.md` files in the directory.
fn update_memory_index(dir: &Path) -> std::io::Result<()> {
    let entries = std::fs::read_dir(dir)?;

    let mut files: Vec<(String, String)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Skip MEMORY.md itself and non-markdown files
        if name == "MEMORY.md" || !name.ends_with(".md") {
            continue;
        }

        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let description = extract_description(&content);
        files.push((name, description));
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut index = String::from("# Memory Index\n");
    for (name, desc) in &files {
        let line = if desc.is_empty() {
            format!("- [{name}]({name})\n")
        } else {
            format!("- [{name}]({name}) — {desc}\n")
        };
        index.push_str(&line);
    }

    // Cap at limits
    let truncated: String = index
        .lines()
        .take(MemoryTool::MAX_INDEX_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let final_content = if truncated.len() > MemoryTool::MAX_INDEX_BYTES {
        &truncated[..MemoryTool::MAX_INDEX_BYTES]
    } else {
        &truncated
    };

    // Atomic write
    let index_path = dir.join("MEMORY.md");
    let tmp_path = dir.join("MEMORY.md.tmp");
    std::fs::write(&tmp_path, final_content)?;
    std::fs::rename(&tmp_path, &index_path)?;

    Ok(())
}

/// Extract a description from file content.
///
/// If the file has YAML frontmatter with a `description:` field, use that.
/// Otherwise, use the first non-empty content line.
fn extract_description(content: &str) -> String {
    let trimmed = content.trim();

    // Check for YAML frontmatter
    if let Some(rest) = trimmed.strip_prefix("---")
        && let Some(end) = rest.find("---")
    {
        let frontmatter = &rest[..end];
        for line in frontmatter.lines() {
            let line = line.trim();
            if let Some(desc) = line.strip_prefix("description:") {
                let desc = desc.trim().trim_matches('"').trim_matches('\'');
                if !desc.is_empty() {
                    return desc.to_string();
                }
            }
        }
    }

    // Fall back to first non-empty, non-heading line
    for line in trimmed.lines() {
        let line = line.trim();
        if !line.is_empty() && !line.starts_with('#') && !line.starts_with("---") {
            return line.to_string();
        }
    }

    String::new()
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
