//! Unified diff patch application — manual fallback when git apply fails.

use std::path::Path;

use opendev_tools_core::ToolResult;

use crate::path_utils::resolve_file_path;

/// Simple manual patch application for when git is not available.
pub(super) fn apply_patch_manually(patch: &str, cwd: &Path, strip: usize) -> ToolResult {
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

pub(super) fn strip_path(path: &str, strip: usize) -> String {
    let parts: Vec<&str> = path.splitn(strip + 1, '/').collect();
    if parts.len() > strip {
        parts[strip..].join("/")
    } else {
        path.to_string()
    }
}

pub(super) struct HunkBuilder {
    pub(super) old_start: usize,
    pub(super) lines: Vec<String>,
}

pub(super) struct Hunk {
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

pub(super) fn parse_hunk_header(line: &str) -> Option<HunkBuilder> {
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
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create directory: {e}"))?;
    }

    let content = file_lines.join("\n");
    // Preserve trailing newline if original had one
    let content = if original.ends_with('\n') && !content.ends_with('\n') {
        content + "\n"
    } else {
        content
    };

    let temp_path = if parent.as_os_str().is_empty() {
        std::path::PathBuf::from(format!(".tmp-{}", uuid::Uuid::new_v4()))
    } else {
        parent.join(format!(".tmp-{}", uuid::Uuid::new_v4()))
    };

    let write_res = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        use std::io::Write;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
        Ok(())
    })();

    match write_res {
        Ok(_) => {
            if let Err(e) = std::fs::rename(&temp_path, &path) {
                let _ = std::fs::remove_file(&temp_path);
                return Err(format!("Cannot rename temp file for {file}: {e}"));
            }
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&temp_path);
            Err(format!("Cannot write {file}: {e}"))
        }
    }
}
