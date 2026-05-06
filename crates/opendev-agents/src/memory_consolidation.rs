//! Memory consolidation ("dream") system.
//!
//! Periodically merges session notes and stale memories into durable topic
//! files. Triggered at session start when:
//! - At least 24 hours since last consolidation
//! - At least 5 `type: session` memory files exist
//! - No concurrent consolidation (lock file guard)
//!
//! The process backs up files before modifying them and never touches
//! `user` or `reference` type memories (those are atomic and personal).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::attachments::collectors::memory_selector::MemorySelector;

/// Minimum number of session files to trigger consolidation.
const MIN_SESSION_FILES: usize = 5;
/// Maximum number of session files to process in one consolidation.
const MAX_FILES_PER_RUN: usize = 20;

/// Consolidation metadata persisted between runs.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ConsolidationMeta {
    /// ISO 8601 timestamp of last consolidation.
    pub last_run: Option<String>,
    /// Number of files processed in last run.
    pub files_processed: usize,
}

/// Result of a consolidation run.
#[derive(Debug)]
pub struct ConsolidationReport {
    pub files_consolidated: usize,
    pub files_pruned: usize,
    pub files_backed_up: usize,
}

/// Check whether consolidation should run.
pub fn should_consolidate(working_dir: &Path) -> bool {
    let paths = opendev_config::paths::Paths::new(Some(working_dir.to_path_buf()));
    let memory_dir = paths.project_memory_dir();

    if !memory_dir.exists() {
        return false;
    }

    // Check lock file
    let lock_path = paths.consolidation_lock_path();
    if lock_path.exists() {
        debug!("Consolidation lock file exists, skipping");
        return false;
    }

    // Check time since last run
    let meta_path = paths.consolidation_meta_path();
    let meta = load_meta(&meta_path);
    if let Some(ref last_run) = meta.last_run
        && let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last_run)
    {
        let elapsed = chrono::Utc::now().signed_duration_since(last_time);
        if elapsed < chrono::Duration::hours(24) {
            debug!(
                "Last consolidation was {} hours ago, skipping",
                elapsed.num_hours()
            );
            return false;
        }
    }

    // Count session files
    let session_count = count_session_files(&memory_dir);
    if session_count < MIN_SESSION_FILES {
        debug!(
            "Only {session_count} session files (need {MIN_SESSION_FILES}), skipping consolidation"
        );
        return false;
    }

    true
}

/// Run the consolidation process.
///
/// Returns a report of what was done, or `None` if consolidation was skipped
/// (e.g., no LLM available for merging).
pub async fn consolidate(working_dir: &Path) -> Option<ConsolidationReport> {
    let paths = opendev_config::paths::Paths::new(Some(working_dir.to_path_buf()));
    let memory_dir = paths.project_memory_dir();
    let lock_path = paths.consolidation_lock_path();
    let meta_path = paths.consolidation_meta_path();
    let backup_dir = paths.memory_backup_dir();

    // Acquire lock
    if let Err(e) = std::fs::write(&lock_path, "locked") {
        warn!("Failed to acquire consolidation lock: {e}");
        return None;
    }

    let result = run_consolidation(&memory_dir, &backup_dir).await;

    // Update meta
    let mut meta = load_meta(&meta_path);
    meta.last_run = Some(chrono::Utc::now().to_rfc3339());
    if let Some(ref report) = result {
        meta.files_processed = report.files_consolidated;
    }
    save_meta(&meta_path, &meta);

    // Release lock
    let _ = std::fs::remove_file(&lock_path);

    result
}

async fn run_consolidation(memory_dir: &Path, backup_dir: &Path) -> Option<ConsolidationReport> {
    // Phase 1: Orient — collect all memory files
    let all_files = scan_all_memory_files(memory_dir);
    let session_files: Vec<&MemoryFile> = all_files
        .iter()
        .filter(|f| f.file_type == "session")
        .take(MAX_FILES_PER_RUN)
        .collect();

    if session_files.is_empty() {
        return None;
    }

    info!(
        "Starting memory consolidation: {} session files to process",
        session_files.len()
    );

    // Phase 2: Backup
    if let Err(e) = std::fs::create_dir_all(backup_dir) {
        warn!("Failed to create backup directory: {e}");
        return None;
    }

    let mut backed_up = 0;
    for file in &session_files {
        let dest = backup_dir.join(&file.filename);
        if let Err(e) = std::fs::copy(&file.path, &dest) {
            warn!("Failed to backup {}: {e}", file.filename);
        } else {
            backed_up += 1;
        }
    }

    // Phase 3: Consolidate — merge session notes into a summary
    let merged_content = merge_session_files(&session_files).await;
    let merged_content = match merged_content {
        Some(c) => c,
        None => {
            // Fallback: simple concatenation when no LLM is available
            let mut combined = String::new();
            for file in &session_files {
                combined.push_str(&format!("## From: {}\n\n", file.filename));
                combined.push_str(&file.body);
                combined.push_str("\n\n---\n\n");
            }
            combined
        }
    };

    // Write consolidated file
    let now = chrono::Local::now();
    let consolidated_filename = format!("consolidated-{}.md", now.format("%Y-%m-%d"));
    let frontmatter = format!(
        "---\n\
         type: project\n\
         description: \"Consolidated session notes from {}\"\n\
         created: {}\n\
         ---\n\n",
        now.format("%Y-%m-%d"),
        now.format("%Y-%m-%d")
    );
    let consolidated_path = memory_dir.join(&consolidated_filename);
    let full_content = format!("{frontmatter}{merged_content}");
    if let Err(e) = std::fs::write(&consolidated_path, &full_content) {
        warn!("Failed to write consolidated file: {e}");
        return None;
    }

    // Phase 4: Prune — remove session files that were consolidated
    let mut pruned = 0;
    for file in &session_files {
        if let Err(e) = std::fs::remove_file(&file.path) {
            warn!(
                "Failed to remove consolidated session file {}: {e}",
                file.filename
            );
        } else {
            pruned += 1;
        }
    }

    // Update MEMORY.md index
    let _ = regenerate_index(memory_dir);

    info!(
        "Consolidation complete: {} files merged, {} pruned, {} backed up",
        session_files.len(),
        pruned,
        backed_up
    );

    Some(ConsolidationReport {
        files_consolidated: session_files.len(),
        files_pruned: pruned,
        files_backed_up: backed_up,
    })
}

/// Use cheap LLM to merge session notes into a coherent summary.
async fn merge_session_files(files: &[&MemoryFile]) -> Option<String> {
    let selector = MemorySelector::try_new()?;

    let mut input = String::from(
        "Merge these session notes into a single coherent summary. \
        Remove duplicates, merge overlapping facts, keep key decisions and learnings. \
        Return ONLY the merged markdown content:\n\n",
    );

    for file in files {
        input.push_str(&format!("### {}\n{}\n\n", file.filename, file.body));
    }

    let prompt = "You merge session notes into a coherent project knowledge summary. \
        Combine overlapping information, remove ephemeral details (specific timestamps, \
        greetings), and keep durable facts: architecture decisions, conventions, \
        bugs found, and learnings. Return a clean markdown document with sections. \
        Return ONLY the markdown, no code fences.";

    match selector
        .select_with_prompt(&input, "Merge session notes", prompt)
        .await
    {
        Ok(result) => result.into_iter().next(),
        Err(e) => {
            warn!("LLM merge failed: {e}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct MemoryFile {
    filename: String,
    path: PathBuf,
    file_type: String,
    body: String,
    modified: SystemTime,
}

fn scan_all_memory_files(dir: &Path) -> Vec<MemoryFile> {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut files = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) if name.ends_with(".md") && name != "MEMORY.md" => name.to_string(),
            _ => continue,
        };

        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let (file_type, body) = parse_type_and_body(&content);

        files.push(MemoryFile {
            filename,
            path,
            file_type,
            body,
            modified,
        });
    }

    // Sort oldest first (process oldest sessions first)
    files.sort_by_key(|a| a.modified);
    files
}

fn parse_type_and_body(content: &str) -> (String, String) {
    let mut file_type = String::from("general");
    let trimmed = content.trim();

    if let Some(rest) = trimmed.strip_prefix("---")
        && let Some(end_idx) = rest.find("---")
    {
        let frontmatter = &rest[..end_idx];
        for line in frontmatter.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("type:") {
                file_type = val.trim().trim_matches('"').trim_matches('\'').to_string();
            }
        }
        let body = &rest[end_idx + 3..];
        return (file_type, body.trim().to_string());
    }

    (file_type, trimmed.to_string())
}

fn count_session_files(dir: &Path) -> usize {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return 0,
    };

    read_dir
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return false;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.ends_with(".md") || name == "MEMORY.md" {
                return false;
            }
            // Quick check: session files start with "session-"
            if name.starts_with("session-") {
                return true;
            }
            // Full check: read frontmatter for type: session
            if let Ok(content) = std::fs::read_to_string(path) {
                let (ft, _) = parse_type_and_body(&content);
                ft == "session"
            } else {
                false
            }
        })
        .count()
}

fn load_meta(path: &Path) -> ConsolidationMeta {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_meta(path: &Path, meta: &ConsolidationMeta) {
    if let Ok(json) = serde_json::to_string_pretty(meta) {
        let _ = std::fs::write(path, json);
    }
}

fn regenerate_index(dir: &Path) -> std::io::Result<()> {
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

        if name == "MEMORY.md" || !name.ends_with(".md") {
            continue;
        }

        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let desc = extract_desc(&content);
        files.push((name, desc));
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut index = String::from("# Memory Index\n");
    for (name, desc) in &files {
        if desc.is_empty() {
            index.push_str(&format!("- [{name}]({name})\n"));
        } else {
            index.push_str(&format!("- [{name}]({name}) \u{2014} {desc}\n"));
        }
    }

    let truncated: String = index.lines().take(200).collect::<Vec<_>>().join("\n");
    let final_content = if truncated.len() > 25 * 1024 {
        &truncated[..25 * 1024]
    } else {
        &truncated
    };

    let index_path = dir.join("MEMORY.md");
    let tmp_path = dir.join("MEMORY.md.tmp");
    std::fs::write(&tmp_path, final_content)?;
    std::fs::rename(&tmp_path, &index_path)?;
    Ok(())
}

fn extract_desc(content: &str) -> String {
    let trimmed = content.trim();
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
    String::new()
}

#[cfg(test)]
#[path = "memory_consolidation_tests.rs"]
mod tests;
