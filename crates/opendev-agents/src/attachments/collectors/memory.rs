//! Semantic memory retrieval: scans individual memory files, uses a cheap LLM
//! side-query to select relevant ones, and injects them as context.
//!
//! Falls back to injecting the full MEMORY.md index when:
//! - No API key is available for the side-query
//! - The LLM call fails or times out
//! - No individual memory files exist (only MEMORY.md)

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use tracing::{debug, warn};

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

/// Cheap models per provider for the selection side-query.
const CHEAP_MODELS: &[(&str, &str)] = &[
    ("openai", "gpt-4o-mini"),
    ("anthropic", "claude-3-5-haiku-20241022"),
    (
        "fireworks",
        "accounts/fireworks/models/llama-v3p1-8b-instruct",
    ),
];

/// Env var names per provider.
const ENV_KEYS: &[(&str, &str)] = &[
    ("openai", "OPENAI_API_KEY"),
    ("anthropic", "ANTHROPIC_API_KEY"),
    ("fireworks", "FIREWORKS_API_KEY"),
];

/// API endpoint per provider.
fn api_endpoint(provider: &str) -> &'static str {
    match provider {
        "fireworks" => "https://api.fireworks.ai/inference/v1/chat/completions",
        _ => "https://api.openai.com/v1/chat/completions",
    }
}

const SELECTION_PROMPT: &str = "\
You select memories relevant to the current coding task. \
Given a list of memory files with descriptions, return a JSON array of filenames \
(max 5) that are most relevant to the user's query. Return [] if none are relevant. \
Only return the JSON array, no other text.";

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
// LLM-based memory selector
// ---------------------------------------------------------------------------

/// Makes a cheap LLM side-query to select relevant memory files.
struct MemorySelector {
    provider: String,
    model: String,
    api_key: String,
    client: reqwest::Client,
}

impl MemorySelector {
    /// Try to create a selector by resolving a cheap model and API key.
    fn try_new() -> Option<Self> {
        // Try each provider in order
        for &(prov, model) in CHEAP_MODELS {
            let env_key = ENV_KEYS
                .iter()
                .find(|&&(p, _)| p == prov)
                .map(|&(_, k)| k)
                .unwrap_or("");
            if let Ok(key) = std::env::var(env_key)
                && !key.is_empty()
            {
                return Some(Self {
                    provider: prov.to_string(),
                    model: model.to_string(),
                    api_key: key,
                    client: reqwest::Client::new(),
                });
            }
        }
        None
    }

    /// Call the LLM to select relevant memory filenames.
    async fn select(
        &self,
        manifest: &str,
        user_query: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = api_endpoint(&self.provider);

        let user_content = format!(
            "Memory files:\n{manifest}\n\nCurrent task: {user_query}"
        );

        let payload = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": SELECTION_PROMPT},
                {"role": "user", "content": user_content},
            ],
            "max_tokens": 200,
            "temperature": 0.0,
        });

        let resp = self
            .client
            .post(endpoint)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("Memory selector API returned {}", resp.status()).into());
        }

        let body: serde_json::Value = resp.json().await?;
        let content = body
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or("No content in memory selector response")?;

        // Parse JSON array of filenames from the response
        let filenames: Vec<String> = serde_json::from_str(content.trim())?;
        Ok(filenames.into_iter().take(MAX_SELECTIONS).collect())
    }
}

// ---------------------------------------------------------------------------
// Collector implementation
// ---------------------------------------------------------------------------

/// Semantic memory collector that selects relevant memory files per query.
///
/// Falls back to the full MEMORY.md index when the LLM side-query is unavailable.
pub struct SemanticMemoryCollector {
    cadence: CadenceGate,
    last_query_hash: AtomicU64,
    selector: Option<MemorySelector>,
    surfaced_files: Mutex<HashSet<String>>,
    cumulative_bytes: AtomicUsize,
}

impl SemanticMemoryCollector {
    pub fn new(interval: usize) -> Self {
        Self {
            cadence: CadenceGate::new(interval),
            last_query_hash: AtomicU64::new(0),
            selector: MemorySelector::try_new(),
            surfaced_files: Mutex::new(HashSet::new()),
            cumulative_bytes: AtomicUsize::new(0),
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
                self.format_selected_memories(ctx.working_dir, &filenames)
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
    fn format_selected_memories(
        &self,
        working_dir: &Path,
        filenames: &[String],
    ) -> Option<String> {
        let mut surfaced = self.surfaced_files.lock().unwrap();
        let cumulative = self.cumulative_bytes.load(Ordering::Relaxed);
        let mut remaining_budget = MAX_SESSION_BYTES.saturating_sub(cumulative);

        let mut sections = Vec::new();
        for filename in filenames {
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
                sections.push(format!("## Memory: {filename}\n\n{content}"));
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

    async fn collect(&self, ctx: &TurnContext<'_>) -> Option<Attachment> {
        let content = self.collect_memories(ctx).await?;

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
    }
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
