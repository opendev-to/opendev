//! Semantic memory retrieval: scans individual memory files, uses a cheap LLM
//! side-query to select relevant ones, and injects them as context.
//!
//! Falls back to injecting the full MEMORY.md index when:
//! - No API key is available for the side-query
//! - The LLM call fails or times out
//! - No individual memory files exist (only MEMORY.md)

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::SystemTime;

use tracing::{debug, warn};

use super::memory_selector::MemorySelector;
use crate::attachments::{Attachment, CadenceGate, ContextCollector, TurnContext};
use crate::prompts::reminders::MessageClass;

/// Maximum memory content size for MEMORY.md fallback (25 KB).
const MAX_MEMORY_BYTES: usize = 25 * 1024;
/// Maximum lines for MEMORY.md fallback.
const MAX_MEMORY_LINES: usize = 200;
/// Maximum lines per individual memory file.
const MAX_FILE_LINES: usize = 200;
/// Maximum bytes per individual memory file.
const MAX_FILE_BYTES: usize = 4096;
/// Maximum number of memories to select per query.
const MAX_SELECTIONS: usize = 5;
/// Cumulative byte limit per session (60 KB).
const MAX_SESSION_BYTES: usize = 60 * 1024;

// ---------------------------------------------------------------------------
// Memory file entry & manifest scanning
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct MemoryFileEntry {
    filename: String,
    description: String,
    file_type: String,
    modified: SystemTime,
}

/// Scan the project memory directory for individual `.md` files (excluding MEMORY.md).
fn scan_memory_dir(working_dir: &Path) -> Vec<MemoryFileEntry> {
    let paths = opendev_config::paths::Paths::new(Some(working_dir.to_path_buf()));
    let memory_dir = paths.project_memory_dir();

    let read_dir = match std::fs::read_dir(&memory_dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
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

        // Parse frontmatter (first 30 lines max)
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let (description, file_type) = parse_frontmatter(&content);

        entries.push(MemoryFileEntry {
            filename,
            description,
            file_type,
            modified,
        });
    }

    // Sort by modification time, newest first
    entries.sort_by(|a, b| b.modified.cmp(&a.modified));
    entries
}

/// Extract `description` and `type` from YAML frontmatter.
fn parse_frontmatter(content: &str) -> (String, String) {
    let mut description = String::new();
    let mut file_type = String::from("general");

    let mut in_frontmatter = false;
    for (i, line) in content.lines().enumerate() {
        if i == 0 && line.trim() == "---" {
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            if line.trim() == "---" {
                break;
            }
            if let Some(val) = line.strip_prefix("description:") {
                description = val.trim().trim_matches('"').to_string();
            } else if let Some(val) = line.strip_prefix("type:") {
                file_type = val.trim().trim_matches('"').to_string();
            }
        }
        if i > 30 {
            break;
        }
    }

    (description, file_type)
}

/// Format memory entries as a manifest string for the selection prompt.
fn format_manifest(entries: &[MemoryFileEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            let date = e
                .modified
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| {
                    let secs = d.as_secs();
                    let days_ago = (SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .saturating_sub(secs))
                        / 86400;
                    if days_ago == 0 {
                        "today".to_string()
                    } else if days_ago == 1 {
                        "1 day ago".to_string()
                    } else {
                        format!("{days_ago} days ago")
                    }
                })
                .unwrap_or_else(|_| "unknown".to_string());

            let desc = if e.description.is_empty() {
                "(no description)".to_string()
            } else {
                e.description.clone()
            };
            format!("- [{}] {} ({}): {}", e.file_type, e.filename, date, desc)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Read a memory file's content, truncated to limits.
fn read_memory_file(working_dir: &Path, filename: &str) -> Option<String> {
    let paths = opendev_config::paths::Paths::new(Some(working_dir.to_path_buf()));
    let file_path = paths.project_memory_dir().join(filename);
    let content = std::fs::read_to_string(&file_path).ok()?;
    if content.trim().is_empty() {
        return None;
    }

    let truncated: String = content
        .lines()
        .take(MAX_FILE_LINES)
        .collect::<Vec<_>>()
        .join("\n");

    if truncated.len() > MAX_FILE_BYTES {
        Some(truncated[..MAX_FILE_BYTES].to_string())
    } else {
        Some(truncated)
    }
}

// ---------------------------------------------------------------------------
// Staleness tracking
// ---------------------------------------------------------------------------

/// Return a human-readable staleness annotation for a memory file.
///
/// - Less than 1 day: no annotation
/// - 1–7 days: informational
/// - 7–30 days: may be outdated
/// - Over 30 days: warning to verify
fn staleness_annotation(modified: SystemTime) -> Option<String> {
    let days_ago = SystemTime::now()
        .duration_since(modified)
        .ok()
        .map(|d| d.as_secs() / 86400)?;

    match days_ago {
        0 => None,
        1..=6 => Some(format!(
            "Updated {days_ago} day{} ago",
            if days_ago == 1 { "" } else { "s" }
        )),
        7..=30 => Some(format!(
            "Updated {days_ago} days ago \u{2014} may be outdated, verify before acting on this"
        )),
        _ => Some(format!(
            "WARNING: Last updated {days_ago} days ago. Verify against current code before relying on this."
        )),
    }
}

// ---------------------------------------------------------------------------
// Collector implementation
// ---------------------------------------------------------------------------

/// Semantic memory collector that selects relevant memory files per query.
///
/// Falls back to the full MEMORY.md index when the LLM side-query is unavailable.
///
/// Uses a prefetch mechanism: `pre_fire()` spawns the LLM side-query as a
/// background task, and `collect()` awaits its result, overlapping memory
/// retrieval with other pre-turn work.
pub struct SemanticMemoryCollector {
    cadence: CadenceGate,
    last_query_hash: AtomicU64,
    selector: Option<MemorySelector>,
    surfaced_files: Mutex<HashSet<String>>,
    cumulative_bytes: AtomicUsize,
    /// Prefetched result from `pre_fire()`, consumed by `collect()`.
    prefetch_result: Mutex<Option<String>>,
}

impl SemanticMemoryCollector {
    pub fn new(interval: usize) -> Self {
        Self {
            cadence: CadenceGate::new(interval),
            last_query_hash: AtomicU64::new(0),
            selector: MemorySelector::try_new(),
            surfaced_files: Mutex::new(HashSet::new()),
            cumulative_bytes: AtomicUsize::new(0),
            prefetch_result: Mutex::new(None),
        }
    }

    /// Load the full MEMORY.md index (fallback path).
    fn load_memory_index(working_dir: &Path) -> Option<String> {
        let paths = opendev_config::paths::Paths::new(Some(working_dir.to_path_buf()));
        let content = std::fs::read_to_string(paths.project_memory_index()).ok()?;
        if content.trim().is_empty() {
            return None;
        }
        let truncated: String = content
            .lines()
            .take(MAX_MEMORY_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        if truncated.len() > MAX_MEMORY_BYTES {
            Some(truncated[..MAX_MEMORY_BYTES].to_string())
        } else {
            Some(truncated)
        }
    }

    fn hash_content(content: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    /// Try semantic selection; on failure, return MEMORY.md fallback.
    async fn collect_memories(&self, ctx: &TurnContext<'_>) -> Option<String> {
        let entries = scan_memory_dir(ctx.working_dir);

        // If no individual memory files, fall back to MEMORY.md
        if entries.is_empty() {
            return Self::load_memory_index(ctx.working_dir)
                .map(|c| format!("Project memory (MEMORY.md):\n\n{c}"));
        }

        let user_query = match ctx.last_user_query {
            Some(q) if !q.trim().is_empty() => q,
            _ => {
                // No query context — fall back to MEMORY.md
                return Self::load_memory_index(ctx.working_dir)
                    .map(|c| format!("Project memory (MEMORY.md):\n\n{c}"));
            }
        };

        // Hash manifest + query to avoid re-querying unchanged state
        let manifest = format_manifest(&entries);
        let hash_input = format!("{manifest}\n---\n{user_query}");
        let hash = Self::hash_content(&hash_input);
        let prev = self.last_query_hash.load(Ordering::Relaxed);
        if hash == prev {
            return None; // unchanged since last selection
        }
        self.last_query_hash.store(hash, Ordering::Relaxed);

        // Try LLM selection
        let selector = match &self.selector {
            Some(s) => s,
            None => {
                debug!("No API key for memory selector, using MEMORY.md fallback");
                return Self::load_memory_index(ctx.working_dir)
                    .map(|c| format!("Project memory (MEMORY.md):\n\n{c}"));
            }
        };

        match selector.select(&manifest, user_query).await {
            Ok(filenames) if !filenames.is_empty() => {
                // Build (filename, modified) pairs for staleness tracking
                let selections: Vec<(String, SystemTime)> = filenames
                    .into_iter()
                    .take(MAX_SELECTIONS)
                    .map(|name| {
                        let mtime = entries
                            .iter()
                            .find(|e| e.filename == name)
                            .map(|e| e.modified)
                            .unwrap_or(SystemTime::UNIX_EPOCH);
                        (name, mtime)
                    })
                    .collect();
                self.format_selected_memories(ctx.working_dir, &selections)
            }
            Ok(_) => {
                debug!("Memory selector returned no relevant files");
                None
            }
            Err(e) => {
                warn!("Memory selector failed: {e}, falling back to MEMORY.md");
                Self::load_memory_index(ctx.working_dir)
                    .map(|c| format!("Project memory (MEMORY.md):\n\n{c}"))
            }
        }
    }

    /// Read and format selected memory files, respecting dedup and byte limits.
    ///
    /// Each selection is a `(filename, modified_time)` pair for staleness tracking.
    fn format_selected_memories(
        &self,
        working_dir: &Path,
        selections: &[(String, SystemTime)],
    ) -> Option<String> {
        let mut surfaced = self.surfaced_files.lock().unwrap();
        let cumulative = self.cumulative_bytes.load(Ordering::Relaxed);
        let mut remaining_budget = MAX_SESSION_BYTES.saturating_sub(cumulative);

        let mut sections = Vec::new();
        for (filename, modified) in selections {
            if surfaced.contains(filename) {
                continue;
            }
            if remaining_budget == 0 {
                break;
            }

            if let Some(content) = read_memory_file(working_dir, filename) {
                let bytes = content.len();
                if bytes > remaining_budget {
                    break;
                }
                let staleness = staleness_annotation(*modified);
                let header = if let Some(ref note) = staleness {
                    format!("## Memory: {filename}\n\n_{note}_\n\n{content}")
                } else {
                    format!("## Memory: {filename}\n\n{content}")
                };
                sections.push(header);
                surfaced.insert(filename.clone());
                remaining_budget -= bytes;
                self.cumulative_bytes.fetch_add(bytes, Ordering::Relaxed);
            }
        }

        if sections.is_empty() {
            return None;
        }

        Some(format!(
            "Relevant project memories selected for this query:\n\n{}",
            sections.join("\n\n---\n\n")
        ))
    }
}

#[async_trait::async_trait]
impl ContextCollector for SemanticMemoryCollector {
    fn name(&self) -> &'static str {
        "semantic_memory"
    }

    fn should_fire(&self, ctx: &TurnContext<'_>) -> bool {
        // Check session byte budget
        if self.cumulative_bytes.load(Ordering::Relaxed) >= MAX_SESSION_BYTES {
            return false;
        }
        self.cadence.should_fire(ctx.turn_number)
    }

    async fn pre_fire(&self, ctx: &TurnContext<'_>) {
        // Start memory collection early; result is stored for collect() to consume
        let result = self.collect_memories(ctx).await;
        *self.prefetch_result.lock().unwrap() = result;
    }

    async fn collect(&self, _ctx: &TurnContext<'_>) -> Option<Attachment> {
        // Consume the prefetched result (set by pre_fire)
        let content = self.prefetch_result.lock().unwrap().take()?;

        Some(Attachment {
            name: "memory",
            content,
            class: MessageClass::Nudge,
        })
    }

    fn did_fire(&self, turn: usize) {
        self.cadence.mark_fired(turn);
    }

    fn reset(&self) {
        self.cadence.reset();
        self.last_query_hash.store(0, Ordering::Relaxed);
        // Don't reset surfaced_files or cumulative_bytes — they're session-scoped
        *self.prefetch_result.lock().unwrap() = None;
    }
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
