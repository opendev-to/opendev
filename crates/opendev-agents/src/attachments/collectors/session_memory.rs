//! Session memory collector: auto-extracts conversation notes at token
//! thresholds and writes them to persistent memory files.
//!
//! Unlike `SemanticMemoryCollector` (which *reads* memories), this collector
//! *writes* new memories by summarizing the recent conversation. It fires
//! at cumulative token thresholds (50K, 100K, 150K, ...) and produces a
//! session notes file in the project memory directory.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::{debug, warn};

use super::memory_selector::MemorySelector;
use crate::attachments::{Attachment, ContextCollector, TurnContext};
use crate::prompts::reminders::MessageClass;

/// Token threshold interval: extract notes every 50K input tokens.
const TOKEN_THRESHOLD_INTERVAL: u64 = 50_000;
/// Maximum number of recent messages to send for extraction.
const MAX_MESSAGES_FOR_EXTRACTION: usize = 30;
/// Maximum characters of message content to send per message.
const MAX_CHARS_PER_MESSAGE: usize = 2000;

const EXTRACTION_PROMPT: &str = "\
You are a session note-taker. Given recent conversation messages, extract \
a structured summary. Return ONLY the markdown content (no code fences). \
Use these sections, omitting empty ones:\n\
\n\
## Current State\n\
What the user is currently working on.\n\
\n\
## Key Files Modified\n\
Files that were created, edited, or discussed.\n\
\n\
## Errors & Corrections\n\
Bugs found, mistakes corrected, approaches abandoned.\n\
\n\
## Learnings\n\
Non-obvious discoveries, patterns, conventions found.\n\
\n\
## Worklog\n\
Brief chronological list of what was done.";

/// Session memory collector that auto-extracts conversation notes.
pub struct SessionMemoryCollector {
    /// Last token count at which extraction was performed.
    last_extraction_tokens: AtomicU64,
    /// Selector for the cheap LLM side-query.
    selector: Option<MemorySelector>,
    /// Session ID for the notes file, set on first extraction.
    session_file_written: Mutex<bool>,
}

impl Default for SessionMemoryCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionMemoryCollector {
    pub fn new() -> Self {
        Self {
            last_extraction_tokens: AtomicU64::new(0),
            selector: MemorySelector::try_new(),
            session_file_written: Mutex::new(false),
        }
    }

    /// Build a summary of recent messages suitable for the extraction prompt.
    fn summarize_messages(messages: &[serde_json::Value]) -> String {
        let recent = if messages.len() > MAX_MESSAGES_FOR_EXTRACTION {
            &messages[messages.len() - MAX_MESSAGES_FOR_EXTRACTION..]
        } else {
            messages
        };

        let mut summary = String::new();
        for msg in recent {
            let role = msg
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");

            if content.is_empty() {
                continue;
            }

            let truncated = if content.len() > MAX_CHARS_PER_MESSAGE {
                &content[..MAX_CHARS_PER_MESSAGE]
            } else {
                content
            };

            summary.push_str(&format!("[{role}]: {truncated}\n\n"));
        }
        summary
    }

    /// Extract session notes using the cheap LLM.
    async fn extract_notes(&self, messages: &[serde_json::Value]) -> Option<String> {
        let selector = self.selector.as_ref()?;
        let conversation = Self::summarize_messages(messages);
        if conversation.trim().is_empty() {
            return None;
        }

        match selector
            .select_with_prompt(&conversation, "Extract session notes", EXTRACTION_PROMPT)
            .await
        {
            Ok(result) => result.into_iter().next(),
            Err(e) => {
                warn!("Session memory extraction failed: {e}");
                None
            }
        }
    }

    /// Write session notes to disk.
    fn write_notes(working_dir: &Path, session_id: Option<&str>, notes: &str) {
        let paths = opendev_config::paths::Paths::new(Some(working_dir.to_path_buf()));
        let memory_dir = paths.project_memory_dir();

        if let Err(e) = std::fs::create_dir_all(&memory_dir) {
            warn!("Failed to create memory directory: {e}");
            return;
        }

        let now = chrono::Local::now();
        let date_str = now.format("%Y-%m-%d").to_string();
        let sid = session_id.unwrap_or("unknown");
        let short_sid = if sid.len() > 8 { &sid[..8] } else { sid };
        let filename = format!("session-{date_str}-{short_sid}.md");

        let frontmatter = format!(
            "---\n\
             type: session\n\
             description: \"Session notes from {date_str}\"\n\
             session_id: \"{sid}\"\n\
             created: {date_str}\n\
             ---\n\n"
        );

        let content = format!("{frontmatter}{notes}");
        let path = memory_dir.join(&filename);

        match std::fs::write(&path, &content) {
            Ok(_) => debug!(
                "Written session notes to {filename} ({} bytes)",
                content.len()
            ),
            Err(e) => warn!("Failed to write session notes: {e}"),
        }

        // Update MEMORY.md index
        let _ = update_memory_index_after_session(&memory_dir);
    }
}

/// Simplified MEMORY.md index update (reuse logic from MemoryTool).
fn update_memory_index_after_session(dir: &Path) -> std::io::Result<()> {
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
        let description = extract_first_description(&content);
        files.push((name, description));
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

    // Truncate to limits
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

fn extract_first_description(content: &str) -> String {
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

    for line in trimmed.lines() {
        let line = line.trim();
        if !line.is_empty() && !line.starts_with('#') && !line.starts_with("---") {
            return line.to_string();
        }
    }

    String::new()
}

#[async_trait::async_trait]
impl ContextCollector for SessionMemoryCollector {
    fn name(&self) -> &'static str {
        "session_memory"
    }

    fn should_fire(&self, ctx: &TurnContext<'_>) -> bool {
        let tokens = match ctx.cumulative_input_tokens {
            Some(t) => t,
            None => return false,
        };

        let last = self.last_extraction_tokens.load(Ordering::Relaxed);
        // Fire when tokens cross the next threshold
        tokens >= last + TOKEN_THRESHOLD_INTERVAL
    }

    async fn collect(&self, ctx: &TurnContext<'_>) -> Option<Attachment> {
        let messages = ctx.recent_messages?;
        if messages.is_empty() {
            return None;
        }

        let tokens = ctx.cumulative_input_tokens.unwrap_or(0);

        debug!(
            "Session memory extraction triggered at {tokens} tokens (last: {})",
            self.last_extraction_tokens.load(Ordering::Relaxed)
        );

        // Extract notes via cheap LLM
        if let Some(notes) = self.extract_notes(messages).await {
            // Write to disk (side effect)
            Self::write_notes(ctx.working_dir, ctx.session_id, &notes);

            // Update token checkpoint
            self.last_extraction_tokens.store(tokens, Ordering::Relaxed);
            *self.session_file_written.lock().unwrap() = true;

            // Return a brief notification (not the full notes — those are on disk)
            Some(Attachment {
                name: "session_memory",
                content: "Session notes have been auto-saved to memory.".to_string(),
                class: MessageClass::Nudge,
            })
        } else {
            // Still update checkpoint to avoid retrying immediately
            self.last_extraction_tokens.store(tokens, Ordering::Relaxed);
            None
        }
    }

    fn reset(&self) {
        // Don't reset token checkpoint — it's session-scoped
    }
}

#[cfg(test)]
#[path = "session_memory_tests.rs"]
mod tests;
