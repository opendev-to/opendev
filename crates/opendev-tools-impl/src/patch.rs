//! Patch tool — apply unified diff and structured (apply_patch) patches to files.

use std::collections::HashMap;
use std::path::Path;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::diagnostics_helper;
use crate::path_utils::resolve_file_path;

/// Tool for applying unified diff patches.
#[derive(Debug)]
pub struct PatchTool;

#[async_trait::async_trait]
impl BaseTool for PatchTool {
    fn name(&self) -> &str {
        "patch"
    }

    fn description(&self) -> &str {
        "Apply a unified diff or structured patch to files in the working directory."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "Patch content in unified diff or structured (*** Begin Patch) format"
                },
                "strip": {
                    "type": "integer",
                    "description": "Number of leading path components to strip (default: 1)"
                }
            },
            "required": ["patch"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let patch_content = match args.get("patch").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::fail("patch is required"),
        };

        let strip = args.get("strip").and_then(|v| v.as_u64()).unwrap_or(1) as usize;

        let cwd = &ctx.working_dir;

        // Detect structured patch format (*** Begin Patch)
        let mut result = if is_structured_patch(patch_content) {
            apply_structured_patch(patch_content, cwd)
        } else {
            // Try git apply first
            let git_result = try_git_apply(patch_content, cwd, strip).await;
            if git_result.success {
                git_result
            } else {
                // Fall back to manual patch application
                apply_patch_manually(patch_content, cwd, strip)
            }
        };

        // Collect LSP diagnostics for modified files after successful patch
        if result.success
            && let Some(output) = &result.output.clone()
        {
            let modified_files = extract_modified_files(output, cwd);
            let paths: Vec<&Path> = modified_files.iter().map(|p| p.as_path()).collect();
            if !paths.is_empty()
                && let Some(diag_output) =
                    diagnostics_helper::collect_multi_file_diagnostics(ctx, &paths).await
                && let Some(ref mut out) = result.output
            {
                out.push_str(&diag_output);
            }
        }

        result
    }
}

async fn try_git_apply(patch: &str, cwd: &Path, strip: usize) -> ToolResult {
    let strip_arg = format!("-p{strip}");

    let mut child = match tokio::process::Command::new("git")
        .args(["apply", &strip_arg, "--stat", "-"])
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return ToolResult::fail("git not available"),
    };

    // Write patch to stdin
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(patch.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => return ToolResult::fail(format!("git apply failed: {e}")),
    };

    // Now actually apply (first was just --stat for preview)
    let mut child = match tokio::process::Command::new("git")
        .args(["apply", &strip_arg, "-"])
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return ToolResult::fail("git not available"),
    };

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(patch.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let apply_output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => return ToolResult::fail(format!("git apply failed: {e}")),
    };

    if apply_output.status.success() {
        let stat = String::from_utf8_lossy(&output.stdout).to_string();
        ToolResult::ok(format!("Patch applied successfully via git apply.\n{stat}"))
    } else {
        let stderr = String::from_utf8_lossy(&apply_output.stderr).to_string();
        ToolResult::fail(format!("git apply failed: {stderr}"))
    }
}

/// Simple manual patch application for when git is not available.
fn apply_patch_manually(patch: &str, cwd: &Path, strip: usize) -> ToolResult {
    let mut files_modified = Vec::new();
    let mut current_file: Option<String> = None;
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut current_hunk: Option<HunkBuilder> = None;

    for line in patch.lines() {
        if line.starts_with("+++ ") {
            // Save previous file's hunks
            if let Some(file) = current_file.take() {
                if let Err(e) = apply_hunks(cwd, &file, &hunks) {
                    return ToolResult::fail(format!("Failed to patch {file}: {e}"));
                }
                files_modified.push(file);
                hunks.clear();
            }

            // Parse target file path
            let path = line.strip_prefix("+++ ").unwrap_or("");
            let path = strip_path(path, strip);
            if path == "/dev/null" {
                continue;
            }
            current_file = Some(path);

            // Flush any pending hunk
            if let Some(hb) = current_hunk.take() {
                hunks.push(hb.build());
            }
        } else if line.starts_with("@@ ") {
            // Flush previous hunk
            if let Some(hb) = current_hunk.take() {
                hunks.push(hb.build());
            }
            // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
            if let Some(hb) = parse_hunk_header(line) {
                current_hunk = Some(hb);
            }
        } else if let Some(ref mut hb) = current_hunk {
            hb.lines.push(line.to_string());
        }
    }

    // Flush last hunk and file
    if let Some(hb) = current_hunk.take() {
        hunks.push(hb.build());
    }
    if let Some(file) = current_file.take() {
        if let Err(e) = apply_hunks(cwd, &file, &hunks) {
            return ToolResult::fail(format!("Failed to patch {file}: {e}"));
        }
        files_modified.push(file);
    }

    if files_modified.is_empty() {
        return ToolResult::fail("No files were modified by the patch");
    }

    ToolResult::ok(format!(
        "Patch applied manually to {} file(s): {}",
        files_modified.len(),
        files_modified.join(", ")
    ))
}

fn strip_path(path: &str, strip: usize) -> String {
    let parts: Vec<&str> = path.splitn(strip + 1, '/').collect();
    if parts.len() > strip {
        parts[strip..].join("/")
    } else {
        path.to_string()
    }
}

struct HunkBuilder {
    old_start: usize,
    lines: Vec<String>,
}

struct Hunk {
    old_start: usize,
    lines: Vec<String>,
}

impl HunkBuilder {
    fn build(self) -> Hunk {
        Hunk {
            old_start: self.old_start,
            lines: self.lines,
        }
    }
}

fn parse_hunk_header(line: &str) -> Option<HunkBuilder> {
    // @@ -old_start,old_count +new_start,new_count @@
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let old_range = parts[1].strip_prefix('-')?;
    let old_start: usize = old_range.split(',').next()?.parse().ok()?;

    Some(HunkBuilder {
        old_start,
        lines: Vec::new(),
    })
}

fn apply_hunks(cwd: &Path, file: &str, hunks: &[Hunk]) -> Result<(), String> {
    let path = resolve_file_path(file, cwd);

    let original = if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| format!("Cannot read {file}: {e}"))?
    } else {
        String::new()
    };

    let mut file_lines: Vec<String> = original.lines().map(String::from).collect();
    let mut offset: i64 = 0;

    for hunk in hunks {
        let start = ((hunk.old_start as i64 - 1) + offset).max(0) as usize;
        let mut pos = start;
        let mut added = 0i64;
        let mut removed = 0i64;

        for line in &hunk.lines {
            if let Some(content) = line.strip_prefix('+') {
                file_lines.insert(pos, content.to_string());
                pos += 1;
                added += 1;
            } else if let Some(_content) = line.strip_prefix('-') {
                if pos < file_lines.len() {
                    file_lines.remove(pos);
                    removed += 1;
                }
            } else if line.starts_with(' ') || line.is_empty() {
                // Context line — just advance
                pos += 1;
            }
        }

        offset += added - removed;
    }

    // Write result
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create directory: {e}"))?;
    }

    let content = file_lines.join("\n");
    // Preserve trailing newline if original had one
    let content = if original.ends_with('\n') && !content.ends_with('\n') {
        content + "\n"
    } else {
        content
    };

    std::fs::write(&path, content).map_err(|e| format!("Cannot write {file}: {e}"))
}

// ---------------------------------------------------------------------------
// Structured patch format (*** Begin Patch / *** End Patch)
// ---------------------------------------------------------------------------

/// Returns true if the patch content uses the structured patch format.
fn is_structured_patch(patch: &str) -> bool {
    // Check the first 5 non-empty lines for the marker
    let trimmed = patch.trim_start();
    trimmed.starts_with("*** Begin Patch")
        || patch.lines().take(5).any(|l| l.trim() == "*** Begin Patch")
}

/// A single operation in a structured patch.
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
enum PatchOp {
    AddFile { path: String, content: String },
    DeleteFile { path: String },
    MoveFile { old_path: String, new_path: String },
    UpdateFile { path: String, changes: Vec<String> },
}

/// Parse structured patch content into a list of operations.
fn parse_structured_patch(patch: &str) -> Result<Vec<PatchOp>, String> {
    let mut ops = Vec::new();
    let lines: Vec<&str> = patch.lines().collect();
    let mut i = 0;

    // Find *** Begin Patch
    while i < lines.len() {
        if lines[i].trim() == "*** Begin Patch" {
            i += 1;
            break;
        }
        i += 1;
    }

    if i >= lines.len() && !lines.iter().any(|l| l.trim() == "*** Begin Patch") {
        return Err("Missing *** Begin Patch marker".to_string());
    }

    while i < lines.len() {
        let line = lines[i].trim();

        if line == "*** End Patch" {
            break;
        }

        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let path = path.trim().to_string();
            i += 1;
            let mut content_lines = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                if l.starts_with("*** ") {
                    break;
                }
                content_lines.push(l);
                i += 1;
            }
            let content = if content_lines.is_empty() {
                String::new()
            } else {
                let mut s = content_lines.join("\n");
                s.push('\n');
                s
            };
            ops.push(PatchOp::AddFile { path, content });
        } else if let Some(path) = line.strip_prefix("*** Delete File: ") {
            ops.push(PatchOp::DeleteFile {
                path: path.trim().to_string(),
            });
            i += 1;
        } else if let Some(rest) = line.strip_prefix("*** Move File: ") {
            if let Some((old, new)) = rest.split_once(" -> ") {
                ops.push(PatchOp::MoveFile {
                    old_path: old.trim().to_string(),
                    new_path: new.trim().to_string(),
                });
            } else {
                return Err(format!("Invalid Move File syntax: {line}"));
            }
            i += 1;
        } else if let Some(path) = line.strip_prefix("*** Update File: ") {
            let path = path.trim().to_string();
            i += 1;
            let mut change_lines = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                if l.starts_with("*** ") {
                    break;
                }
                change_lines.push(l.to_string());
                i += 1;
            }
            ops.push(PatchOp::UpdateFile {
                path,
                changes: change_lines,
            });
        } else {
            // Skip unrecognized lines
            i += 1;
        }
    }

    Ok(ops)
}

/// Apply a structured patch to the working directory.
fn apply_structured_patch(patch: &str, cwd: &Path) -> ToolResult {
    let ops = match parse_structured_patch(patch) {
        Ok(ops) => ops,
        Err(e) => return ToolResult::fail(format!("Failed to parse structured patch: {e}")),
    };

    if ops.is_empty() {
        return ToolResult::fail("No operations found in structured patch");
    }

    let mut summary = Vec::new();

    for op in &ops {
        match op {
            PatchOp::AddFile { path, content } => {
                let full = resolve_file_path(path, cwd);
                if let Err(e) = ensure_parent(&full) {
                    return ToolResult::fail(format!("Cannot create directory for {path}: {e}"));
                }
                if let Err(e) = std::fs::write(&full, content) {
                    return ToolResult::fail(format!("Cannot write {path}: {e}"));
                }
                summary.push(format!("A {path}"));
            }
            PatchOp::DeleteFile { path } => {
                let full = resolve_file_path(path, cwd);
                if full.exists()
                    && let Err(e) = std::fs::remove_file(&full)
                {
                    return ToolResult::fail(format!("Cannot delete {path}: {e}"));
                }
                summary.push(format!("D {path}"));
            }
            PatchOp::MoveFile { old_path, new_path } => {
                let old_full = resolve_file_path(old_path, cwd);
                let new_full = resolve_file_path(new_path, cwd);
                if let Err(e) = ensure_parent(&new_full) {
                    return ToolResult::fail(format!(
                        "Cannot create directory for {new_path}: {e}"
                    ));
                }
                // Copy content then delete old
                match std::fs::read(&old_full) {
                    Ok(data) => {
                        if let Err(e) = std::fs::write(&new_full, &data) {
                            return ToolResult::fail(format!("Cannot write {new_path}: {e}"));
                        }
                        if let Err(e) = std::fs::remove_file(&old_full) {
                            return ToolResult::fail(format!("Cannot remove {old_path}: {e}"));
                        }
                    }
                    Err(e) => {
                        return ToolResult::fail(format!("Cannot read {old_path}: {e}"));
                    }
                }
                summary.push(format!("R {old_path} -> {new_path}"));
            }
            PatchOp::UpdateFile { path, changes } => {
                let full = resolve_file_path(path, cwd);
                let content = match std::fs::read_to_string(&full) {
                    Ok(c) => c,
                    Err(e) => {
                        return ToolResult::fail(format!("Cannot read {path}: {e}"));
                    }
                };
                match apply_context_changes(&content, changes) {
                    Ok(new_content) => {
                        if let Err(e) = std::fs::write(&full, &new_content) {
                            return ToolResult::fail(format!("Cannot write {path}: {e}"));
                        }
                    }
                    Err(e) => {
                        return ToolResult::fail(format!("Failed to update {path}: {e}"));
                    }
                }
                summary.push(format!("M {path}"));
            }
        }
    }

    ToolResult::ok(format!(
        "Structured patch applied ({} operation(s)):\n{}",
        summary.len(),
        summary.join("\n")
    ))
}

/// Extract modified file paths from patch tool output.
///
/// Parses output messages like "A path", "M path", "D path" from structured
/// patches, or file lists from manual patches.
fn extract_modified_files(output: &str, cwd: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        // Structured patch output: "A path", "M path", "R old -> new"
        if let Some(path) = trimmed
            .strip_prefix("A ")
            .or_else(|| trimmed.strip_prefix("M "))
        {
            files.push(cwd.join(path.trim()));
        } else if let Some(rest) = trimmed.strip_prefix("R ") {
            // Move: "R old -> new" — the new file is the one that exists
            if let Some((_, new_path)) = rest.split_once(" -> ") {
                files.push(cwd.join(new_path.trim()));
            }
        }
    }
    files
}

/// Ensure the parent directory of a path exists.
fn ensure_parent(path: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// A group of contiguous changes (context + removals + additions) at one location.
#[derive(Debug)]
struct ChangeGroup {
    /// Context lines to search for (to find the location).
    context_before: Vec<String>,
    /// Lines to remove (without the `-` prefix).
    removals: Vec<String>,
    /// Lines to add (without the `+` prefix).
    additions: Vec<String>,
    /// Context lines after the change (used for validation).
    #[allow(dead_code)]
    context_after: Vec<String>,
}

/// Parse change lines into groups separated by blank-line boundaries.
fn parse_change_groups(changes: &[String]) -> Vec<ChangeGroup> {
    let mut groups: Vec<ChangeGroup> = Vec::new();
    let mut context_before: Vec<String> = Vec::new();
    let mut removals: Vec<String> = Vec::new();
    let mut additions: Vec<String> = Vec::new();
    let mut had_changes = false;

    for line in changes {
        if let Some(removed) = line.strip_prefix('-') {
            removals.push(removed.to_string());
            had_changes = true;
        } else if let Some(added) = line.strip_prefix('+') {
            additions.push(added.to_string());
            had_changes = true;
        } else {
            // Context line: either starts with ' ' or is a blank line
            let ctx = if let Some(stripped) = line.strip_prefix(' ') {
                stripped.to_string()
            } else {
                // Blank line acts as context
                line.to_string()
            };

            if had_changes {
                // This context line comes after changes — flush the group
                // with this as context_after, then start new context_before
                let group = ChangeGroup {
                    context_before: std::mem::take(&mut context_before),
                    removals: std::mem::take(&mut removals),
                    additions: std::mem::take(&mut additions),
                    context_after: vec![ctx.clone()],
                };
                groups.push(group);
                had_changes = false;
                // This context line also starts the next group's context_before
                context_before.push(ctx);
            } else {
                context_before.push(ctx);
            }
        }
    }

    // Flush final group if there were changes
    if had_changes {
        groups.push(ChangeGroup {
            context_before,
            removals,
            additions,
            context_after: Vec::new(),
        });
    }

    groups
}

/// Apply context-based changes to file content.
fn apply_context_changes(content: &str, changes: &[String]) -> Result<String, String> {
    let groups = parse_change_groups(changes);
    if groups.is_empty() {
        return Ok(content.to_string());
    }

    let mut file_lines: Vec<String> = content.lines().map(String::from).collect();
    let had_trailing_newline = content.ends_with('\n');

    for group in &groups {
        let search_lines: Vec<&str> = group
            .context_before
            .iter()
            .chain(group.removals.iter())
            .map(|s| s.as_str())
            .collect();

        if search_lines.is_empty() {
            // No context or removals — append additions at end
            for line in &group.additions {
                file_lines.push(line.clone());
            }
            continue;
        }

        // Find position where search_lines match in file_lines
        let pos = find_context_match(&file_lines, &search_lines).ok_or_else(|| {
            let preview: Vec<&str> = search_lines.iter().take(3).copied().collect();
            format!(
                "Could not find context in file (looking for: {:?}...)",
                preview
            )
        })?;

        // Position after context_before is where removals start
        let removal_start = pos + group.context_before.len();

        // Verify and remove the lines
        for (j, removal) in group.removals.iter().enumerate() {
            let idx = removal_start + j;
            if idx >= file_lines.len() {
                return Err(format!(
                    "Removal line out of bounds at index {idx}: {removal}"
                ));
            }
            // Verify the line matches (with flexible matching)
            if !lines_match(&file_lines[idx], removal) {
                return Err(format!(
                    "Removal mismatch at line {}: expected {:?}, got {:?}",
                    idx + 1,
                    removal,
                    file_lines[idx]
                ));
            }
        }

        // Remove the old lines and insert new ones
        let remove_count = group.removals.len();
        for _ in 0..remove_count {
            if removal_start < file_lines.len() {
                file_lines.remove(removal_start);
            }
        }
        for (j, addition) in group.additions.iter().enumerate() {
            file_lines.insert(removal_start + j, addition.clone());
        }
    }

    let mut result = file_lines.join("\n");
    if had_trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
}

/// Try to find where `needle` lines appear in `haystack` lines.
/// Uses multi-pass: exact match, then trim-end match, then full trim match.
fn find_context_match(haystack: &[String], needle: &[&str]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }

    // Pass 1: exact match
    if let Some(pos) = find_lines_exact(haystack, needle) {
        return Some(pos);
    }

    // Pass 2: trim-end match (trailing whitespace differences)
    if let Some(pos) = find_lines_trimmed(haystack, needle, |s| s.trim_end()) {
        return Some(pos);
    }

    // Pass 3: full trim match
    find_lines_trimmed(haystack, needle, |s| s.trim())
}

fn find_lines_exact(haystack: &[String], needle: &[&str]) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    (0..=(haystack.len() - needle.len())).find(|&i| {
        needle
            .iter()
            .enumerate()
            .all(|(j, n)| haystack[i + j] == *n)
    })
}

fn find_lines_trimmed(
    haystack: &[String],
    needle: &[&str],
    trim_fn: fn(&str) -> &str,
) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    (0..=(haystack.len() - needle.len())).find(|&i| {
        needle
            .iter()
            .enumerate()
            .all(|(j, n)| trim_fn(&haystack[i + j]) == trim_fn(n))
    })
}

/// Check if two lines match (allowing trailing whitespace differences).
fn lines_match(actual: &str, expected: &str) -> bool {
    actual == expected
        || actual.trim_end() == expected.trim_end()
        || actual.trim() == expected.trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_strip_path() {
        assert_eq!(strip_path("a/b/c.rs", 1), "b/c.rs");
        assert_eq!(strip_path("a/b/c.rs", 2), "c.rs");
        assert_eq!(strip_path("c.rs", 0), "c.rs");
    }

    #[test]
    fn test_parse_hunk_header() {
        let hb = parse_hunk_header("@@ -10,5 +10,7 @@ fn main()").unwrap();
        assert_eq!(hb.old_start, 10);
    }

    #[tokio::test]
    async fn test_patch_missing() {
        let tool = PatchTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(!result.success);
    }

    #[test]
    fn test_apply_hunks_simple() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("test.txt"), "line1\nline2\nline3\n").unwrap();

        let hunk = Hunk {
            old_start: 2,
            lines: vec!["-line2".to_string(), "+line2_modified".to_string()],
        };

        apply_hunks(tmp.path(), "test.txt", &[hunk]).unwrap();
        let result = std::fs::read_to_string(tmp.path().join("test.txt")).unwrap();
        assert!(result.contains("line2_modified"));
        assert!(!result.contains("\nline2\n"));
    }

    // -----------------------------------------------------------------------
    // Property-based tests for patch hunk parsing (fuzzing #71)
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Structured patch format tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_structured_patch() {
        assert!(is_structured_patch("*** Begin Patch\n*** End Patch"));
        assert!(is_structured_patch(
            "\n*** Begin Patch\n*** Add File: foo.rs\n*** End Patch"
        ));
        assert!(!is_structured_patch("--- a/foo.rs\n+++ b/foo.rs\n"));
        assert!(!is_structured_patch("random text"));
    }

    #[test]
    fn test_structured_add_file() {
        let tmp = TempDir::new().unwrap();
        let patch = "\
*** Begin Patch
*** Add File: src/new_file.rs
fn main() {
    println!(\"hello\");
}
*** End Patch";

        let result = apply_structured_patch(patch, tmp.path());
        assert!(result.success, "Failed: {:?}", result.output);

        let content = std::fs::read_to_string(tmp.path().join("src/new_file.rs")).unwrap();
        assert!(content.contains("fn main()"));
        assert!(content.contains("println!(\"hello\")"));
    }

    #[test]
    fn test_structured_delete_file() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("old.txt"), "content").unwrap();

        let patch = "\
*** Begin Patch
*** Delete File: old.txt
*** End Patch";

        let result = apply_structured_patch(patch, tmp.path());
        assert!(result.success, "Failed: {:?}", result.output);
        assert!(!tmp.path().join("old.txt").exists());
    }

    #[test]
    fn test_structured_move_file() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("old.rs"), "fn hello() {}").unwrap();

        let patch = "\
*** Begin Patch
*** Move File: old.rs -> subdir/new.rs
*** End Patch";

        let result = apply_structured_patch(patch, tmp.path());
        assert!(result.success, "Failed: {:?}", result.output);
        assert!(!tmp.path().join("old.rs").exists());
        let content = std::fs::read_to_string(tmp.path().join("subdir/new.rs")).unwrap();
        assert_eq!(content, "fn hello() {}");
    }

    #[test]
    fn test_structured_update_file() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("main.rs"),
            "fn main() {\n    println!(\"old\");\n    return;\n}\n",
        )
        .unwrap();

        let patch = "\
*** Begin Patch
*** Update File: main.rs
 fn main() {
-    println!(\"old\");
+    println!(\"new\");
+    println!(\"extra\");
     return;
*** End Patch";

        let result = apply_structured_patch(patch, tmp.path());
        assert!(result.success, "Failed: {:?}", result.output);

        let content = std::fs::read_to_string(tmp.path().join("main.rs")).unwrap();
        assert!(content.contains("println!(\"new\")"));
        assert!(content.contains("println!(\"extra\")"));
        assert!(!content.contains("println!(\"old\")"));
        assert!(content.contains("return;"));
    }

    #[test]
    fn test_structured_update_multiple_locations() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("lib.rs"),
            "use std::io;\n\nfn alpha() {\n    // alpha\n}\n\nfn beta() {\n    // beta\n}\n",
        )
        .unwrap();

        let patch = "\
*** Begin Patch
*** Update File: lib.rs
 fn alpha() {
-    // alpha
+    // alpha modified
 }

 fn beta() {
-    // beta
+    // beta modified
 }
*** End Patch";

        let result = apply_structured_patch(patch, tmp.path());
        assert!(result.success, "Failed: {:?}", result.output);

        let content = std::fs::read_to_string(tmp.path().join("lib.rs")).unwrap();
        assert!(content.contains("// alpha modified"));
        assert!(content.contains("// beta modified"));
        assert!(!content.contains("\n    // alpha\n"));
        assert!(!content.contains("\n    // beta\n"));
    }

    #[test]
    fn test_structured_mixed_operations() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("existing.rs"), "fn existing() {}\n").unwrap();
        std::fs::write(tmp.path().join("to_delete.rs"), "fn old() {}\n").unwrap();

        let patch = "\
*** Begin Patch
*** Add File: new.rs
fn new() {}
*** Delete File: to_delete.rs
*** Update File: existing.rs
-fn existing() {}
+fn existing() { 42 }
*** End Patch";

        let result = apply_structured_patch(patch, tmp.path());
        assert!(result.success, "Failed: {:?}", result.output);

        assert!(tmp.path().join("new.rs").exists());
        assert!(!tmp.path().join("to_delete.rs").exists());
        let content = std::fs::read_to_string(tmp.path().join("existing.rs")).unwrap();
        assert!(content.contains("fn existing() { 42 }"));
    }

    #[test]
    fn test_structured_empty_patch() {
        let tmp = TempDir::new().unwrap();
        let patch = "*** Begin Patch\n*** End Patch";
        let result = apply_structured_patch(patch, tmp.path());
        assert!(!result.success);
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or("")
                .contains("No operations")
        );
    }

    #[test]
    fn test_structured_update_with_trimmed_match() {
        let tmp = TempDir::new().unwrap();
        // File has trailing spaces on a line
        std::fs::write(
            tmp.path().join("spaced.rs"),
            "fn main() {  \n    old_line\n}\n",
        )
        .unwrap();

        let patch = "\
*** Begin Patch
*** Update File: spaced.rs
 fn main() {
-    old_line
+    new_line
 }
*** End Patch";

        let result = apply_structured_patch(patch, tmp.path());
        assert!(result.success, "Failed: {:?}", result.output);

        let content = std::fs::read_to_string(tmp.path().join("spaced.rs")).unwrap();
        assert!(content.contains("new_line"));
    }

    #[test]
    fn test_apply_context_changes_no_changes() {
        let content = "line1\nline2\n";
        let changes: Vec<String> = vec![" line1".to_string(), " line2".to_string()];
        let result = apply_context_changes(content, &changes).unwrap();
        assert_eq!(result, content);
    }

    #[test]
    fn test_parse_change_groups_simple() {
        let changes = vec![
            " context".to_string(),
            "-old".to_string(),
            "+new".to_string(),
        ];
        let groups = parse_change_groups(&changes);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].context_before, vec!["context"]);
        assert_eq!(groups[0].removals, vec!["old"]);
        assert_eq!(groups[0].additions, vec!["new"]);
    }

    #[tokio::test]
    async fn test_execute_routes_structured_patch() {
        let tmp = TempDir::new().unwrap();
        let ctx = ToolContext::new(tmp.path().to_str().unwrap());

        let tool = PatchTool;
        let mut args = HashMap::new();
        args.insert(
            "patch".to_string(),
            serde_json::Value::String(
                "*** Begin Patch\n*** Add File: hello.txt\nhello world\n*** End Patch".to_string(),
            ),
        );

        let result = tool.execute(args, &ctx).await;
        assert!(result.success, "Failed: {:?}", result.output);
        assert!(tmp.path().join("hello.txt").exists());
    }

    mod proptest_patch {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// parse_hunk_header must never panic on arbitrary input.
            #[test]
            fn fuzz_hunk_header_no_panic(line in "\\PC*") {
                let _ = parse_hunk_header(&line);
            }

            /// strip_path must never panic on arbitrary input.
            #[test]
            fn fuzz_strip_path_no_panic(
                path in "\\PC{0,200}",
                strip in 0usize..10
            ) {
                let _ = strip_path(&path, strip);
            }

            /// Valid hunk headers must be parsed correctly.
            #[test]
            fn valid_hunk_header_parsed(
                old_start in 1usize..10000,
                old_count in 0usize..1000,
                new_start in 1usize..10000,
                new_count in 0usize..1000,
            ) {
                let line = format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@");
                let result = parse_hunk_header(&line);
                prop_assert!(result.is_some(), "Failed to parse: {}", line);
                let hb = result.unwrap();
                prop_assert_eq!(hb.old_start, old_start);
            }

            /// apply_patch_manually must not panic on arbitrary patch content.
            #[test]
            fn fuzz_apply_patch_manually_no_panic(
                patch in "\\PC{0,1000}",
                strip in 0usize..5,
            ) {
                let tmp = TempDir::new().unwrap();
                // Create a dummy file so patch application has something to work with
                std::fs::write(tmp.path().join("test.txt"), "line1\nline2\nline3\n").unwrap();
                // Should not panic — errors are returned as ToolResult
                let _ = apply_patch_manually(&patch, tmp.path(), strip);
            }
        }
    }
}
