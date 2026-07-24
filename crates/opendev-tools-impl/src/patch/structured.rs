//! Structured patch format (*** Begin Patch / *** End Patch) application.

use std::path::Path;

use opendev_tools_core::ToolResult;

use crate::path_utils::resolve_file_path;

/// Returns true if the patch content uses the structured patch format.
pub(super) fn is_structured_patch(patch: &str) -> bool {
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
pub(super) fn apply_structured_patch(patch: &str, cwd: &Path) -> ToolResult {
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
                let tmp_path = full.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&tmp_path)
                {
                    Ok(mut f) => {
                        use std::io::Write;
                        if let Err(e) = f.write_all(content.as_bytes()).and_then(|_| f.sync_all()) {
                            let _ = std::fs::remove_file(&tmp_path);
                            return ToolResult::fail(format!("Cannot write {path}: {e}"));
                        }
                        drop(f);
                        if let Err(e) = std::fs::rename(&tmp_path, &full) {
                            let _ = std::fs::remove_file(&tmp_path);
                            return ToolResult::fail(format!(
                                "Cannot rename temporary file for {path}: {e}"
                            ));
                        }
                    }
                    Err(e) => {
                        return ToolResult::fail(format!(
                            "Cannot create temporary file for {path}: {e}"
                        ));
                    }
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
                        let tmp_path =
                            new_full.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
                        match std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&tmp_path)
                        {
                            Ok(mut f) => {
                                use std::io::Write;
                                if let Err(e) = f.write_all(&data).and_then(|_| f.sync_all()) {
                                    let _ = std::fs::remove_file(&tmp_path);
                                    return ToolResult::fail(format!(
                                        "Cannot write {new_path}: {e}"
                                    ));
                                }
                                drop(f);
                                if let Err(e) = std::fs::rename(&tmp_path, &new_full) {
                                    let _ = std::fs::remove_file(&tmp_path);
                                    return ToolResult::fail(format!(
                                        "Cannot rename temporary file for {new_path}: {e}"
                                    ));
                                }
                            }
                            Err(e) => {
                                return ToolResult::fail(format!(
                                    "Cannot create temporary file for {new_path}: {e}"
                                ));
                            }
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
                        let tmp_path = full.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
                        match std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&tmp_path)
                        {
                            Ok(mut f) => {
                                use std::io::Write;
                                if let Err(e) = f
                                    .write_all(new_content.as_bytes())
                                    .and_then(|_| f.sync_all())
                                {
                                    let _ = std::fs::remove_file(&tmp_path);
                                    return ToolResult::fail(format!("Cannot write {path}: {e}"));
                                }
                                drop(f);
                                if let Err(e) = std::fs::rename(&tmp_path, &full) {
                                    let _ = std::fs::remove_file(&tmp_path);
                                    return ToolResult::fail(format!(
                                        "Cannot rename temporary file for {path}: {e}"
                                    ));
                                }
                            }
                            Err(e) => {
                                return ToolResult::fail(format!(
                                    "Cannot create temporary file for {path}: {e}"
                                ));
                            }
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

/// Ensure the parent directory of a path exists.
fn ensure_parent(path: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// A group of contiguous changes (context + removals + additions) at one location.
#[derive(Debug)]
pub(super) struct ChangeGroup {
    /// Context lines to search for (to find the location).
    pub(super) context_before: Vec<String>,
    /// Lines to remove (without the `-` prefix).
    pub(super) removals: Vec<String>,
    /// Lines to add (without the `+` prefix).
    pub(super) additions: Vec<String>,
    /// Context lines after the change (used for validation).
    #[allow(dead_code)]
    pub(super) context_after: Vec<String>,
}

/// Parse change lines into groups separated by blank-line boundaries.
pub(super) fn parse_change_groups(changes: &[String]) -> Vec<ChangeGroup> {
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
pub(super) fn apply_context_changes(content: &str, changes: &[String]) -> Result<String, String> {
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
