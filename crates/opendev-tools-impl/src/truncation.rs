//! Tool output truncation with temp file overflow.
//!
//! When tool output exceeds size limits (lines or bytes), the full output is
//! saved to a temp file under `~/.opendev/tool-output/` and the agent receives
//! a truncated preview plus the file path for follow-up reads.
//!
//! Mirrors OpenCode's `Truncate` system.

use std::path::{Path, PathBuf};

/// Maximum number of lines before truncation.
pub const MAX_LINES: usize = 2000;

/// Maximum output size in bytes before truncation (50 KB).
pub const MAX_BYTES: usize = 50 * 1024;

/// Retention period for temp output files (7 days).
const RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

/// Maximum size for overflow files (1 MB). Prevents a single tool call
/// from writing unbounded output to disk.
const MAX_OVERFLOW_BYTES: usize = 1_024 * 1_024;

/// Direction from which to keep lines when truncating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncateDirection {
    /// Keep the first N lines (default).
    Head,
    /// Keep the last N lines.
    Tail,
}

/// Result of a truncation attempt.
#[derive(Debug, Clone)]
pub struct TruncateResult {
    /// The (possibly truncated) content to return to the agent.
    pub content: String,
    /// Whether the output was truncated.
    pub truncated: bool,
    /// Path to the full output file, if truncated.
    pub output_path: Option<PathBuf>,
}

/// Get the directory for storing truncated tool output.
pub fn output_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".opendev")
        .join("tool-output")
}

/// Truncate tool output if it exceeds size limits.
///
/// If the output fits within `max_lines` and `max_bytes`, returns it as-is.
/// Otherwise, saves the full output to a temp file and returns a truncated
/// preview with a hint about how to access the full content.
pub fn truncate_output(
    text: &str,
    max_lines: Option<usize>,
    max_bytes: Option<usize>,
    direction: TruncateDirection,
) -> TruncateResult {
    let max_lines = max_lines.unwrap_or(MAX_LINES);
    let max_bytes = max_bytes.unwrap_or(MAX_BYTES);
    let lines: Vec<&str> = text.lines().collect();
    let total_bytes = text.len();

    // No truncation needed.
    if lines.len() <= max_lines && total_bytes <= max_bytes {
        return TruncateResult {
            content: text.to_string(),
            truncated: false,
            output_path: None,
        };
    }

    // Collect lines within limits.
    let mut kept: Vec<&str> = Vec::new();
    let mut bytes = 0usize;
    let mut hit_bytes = false;

    match direction {
        TruncateDirection::Head => {
            for (i, line) in lines.iter().enumerate() {
                if i >= max_lines {
                    break;
                }
                let line_bytes = line.len() + if i > 0 { 1 } else { 0 }; // +1 for \n
                if bytes + line_bytes > max_bytes {
                    hit_bytes = true;
                    break;
                }
                kept.push(line);
                bytes += line_bytes;
            }
        }
        TruncateDirection::Tail => {
            // Iterate from the end.
            for (idx, line) in lines.iter().rev().enumerate() {
                if idx >= max_lines {
                    break;
                }
                let line_bytes = line.len() + if idx > 0 { 1 } else { 0 };
                if bytes + line_bytes > max_bytes {
                    hit_bytes = true;
                    break;
                }
                kept.push(line);
                bytes += line_bytes;
            }
            kept.reverse();
        }
    }

    let removed = if hit_bytes {
        total_bytes - bytes
    } else {
        lines.len() - kept.len()
    };
    let unit = if hit_bytes { "bytes" } else { "lines" };
    let preview = kept.join("\n");

    // Save full output to temp file.
    let dir = output_dir();
    let output_path = match save_overflow(&dir, text) {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!(error = %e, "failed to save truncated tool output");
            None
        }
    };

    let hint = if let Some(ref path) = output_path {
        format!(
            "The tool call succeeded but the output was truncated. Full output saved to: {}\n\
             Use Grep to search the full content or Read with offset/limit to view specific sections.",
            path.display()
        )
    } else {
        "The tool call succeeded but the output was truncated.".to_string()
    };

    let content = match direction {
        TruncateDirection::Head => {
            format!("{preview}\n\n...{removed} {unit} truncated...\n\n{hint}")
        }
        TruncateDirection::Tail => {
            format!("...{removed} {unit} truncated...\n\n{hint}\n\n{preview}")
        }
    };

    TruncateResult {
        content,
        truncated: true,
        output_path,
    }
}

/// Save full output to a uniquely-named file in the overflow directory.
///
/// If `text` exceeds [`MAX_OVERFLOW_BYTES`], the saved file is itself truncated
/// (head 75% + tail 25%) to prevent unbounded disk usage.
fn save_overflow(dir: &Path, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let filename = format!("tool_{timestamp}_{}", &id[..8]);
    let filepath = dir.join(filename);

    let to_write = if text.len() > MAX_OVERFLOW_BYTES {
        let head_size = MAX_OVERFLOW_BYTES * 3 / 4;
        let tail_size = MAX_OVERFLOW_BYTES - head_size;
        let head: String = text.chars().take(head_size).collect();
        let tail: String = text
            .chars()
            .rev()
            .take(tail_size)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let omitted = text.len() - head_size - tail_size;
        format!("{head}\n\n[... {omitted} bytes omitted from overflow file ...]\n\n{tail}")
    } else {
        text.to_string()
    };

    std::fs::write(&filepath, &to_write)?;
    Ok(filepath)
}

/// Clean up overflow files older than the retention period (7 days).
///
/// Call this periodically (e.g., on startup or hourly) to prevent unbounded
/// disk usage.
pub fn cleanup_old_files() {
    let dir = output_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return, // Directory doesn't exist yet — nothing to clean.
    };

    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(RETENTION_SECS))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    let mut cleaned = 0u32;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("tool_") {
            continue;
        }
        // Check file modification time.
        if let Ok(meta) = entry.metadata()
            && let Ok(mtime) = meta.modified()
            && mtime < cutoff
            && std::fs::remove_file(entry.path()).is_ok()
        {
            cleaned += 1;
        }
    }
    if cleaned > 0 {
        tracing::debug!(count = cleaned, "cleaned up old tool output files");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_truncation_small_output() {
        let text = "line 1\nline 2\nline 3";
        let result = truncate_output(text, None, None, TruncateDirection::Head);
        assert!(!result.truncated);
        assert_eq!(result.content, text);
        assert!(result.output_path.is_none());
    }

    #[test]
    fn test_truncation_by_lines() {
        let text = (0..10)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_output(&text, Some(3), Some(100_000), TruncateDirection::Head);
        assert!(result.truncated);
        assert!(result.content.contains("line 0"));
        assert!(result.content.contains("line 2"));
        assert!(result.content.contains("truncated"));
        assert!(result.output_path.is_some());

        // Clean up.
        if let Some(p) = &result.output_path {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn test_truncation_by_bytes() {
        // Each line is ~10 bytes. 50 bytes limit should keep ~5 lines.
        let text = (0..20)
            .map(|i| format!("line {i:04}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_output(&text, Some(100), Some(50), TruncateDirection::Head);
        assert!(result.truncated);
        assert!(result.content.contains("bytes truncated"));
        assert!(result.output_path.is_some());

        if let Some(p) = &result.output_path {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn test_truncation_tail_direction() {
        let text = (0..10)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_output(&text, Some(3), Some(100_000), TruncateDirection::Tail);
        assert!(result.truncated);
        // Tail keeps the last 3 lines.
        assert!(result.content.contains("line 9"));
        assert!(result.content.contains("line 8"));
        assert!(result.content.contains("line 7"));
        // First lines should not be in the preview portion.
        let parts: Vec<&str> = result.content.splitn(2, "truncated").collect();
        let after_hint = parts.get(1).unwrap_or(&"");
        assert!(after_hint.contains("line 9"));

        if let Some(p) = &result.output_path {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn test_overflow_file_contains_full_output() {
        let text = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_output(&text, Some(5), Some(100_000), TruncateDirection::Head);
        assert!(result.truncated);
        let path = result.output_path.unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert_eq!(saved, text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_output_path_in_hint() {
        let text = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_output(&text, Some(5), Some(100_000), TruncateDirection::Head);
        assert!(result.truncated);
        let path = result.output_path.as_ref().unwrap();
        assert!(
            result.content.contains(&path.display().to_string()),
            "Hint should contain the file path"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_cleanup_keeps_recent_files() {
        // Files just created should NOT be cleaned up.
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let recent = dir_path.join("tool_9999_abcd1234");
        std::fs::write(&recent, "data").unwrap();
        let non_tool = dir_path.join("other_file.txt");
        std::fs::write(&non_tool, "data").unwrap();

        // Run cleanup logic against this directory (inline since cleanup_old_files
        // uses the real output_dir).
        let entries = std::fs::read_dir(dir_path).unwrap();
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(RETENTION_SECS))
            .unwrap();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("tool_") {
                continue;
            }
            if let Ok(meta) = entry.metadata()
                && let Ok(mtime) = meta.modified()
                && mtime < cutoff
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }

        assert!(recent.exists(), "Recent tool file should be kept");
        assert!(non_tool.exists(), "Non-tool file should be untouched");
    }
}
