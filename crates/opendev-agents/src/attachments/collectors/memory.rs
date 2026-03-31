//! Injects project MEMORY.md content periodically, picking up mid-session changes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::attachments::{Attachment, CadenceGate, ContextCollector, TurnContext};
use crate::prompts::reminders::MessageClass;

/// Maximum memory content size (25 KB).
const MAX_MEMORY_BYTES: usize = 25 * 1024;
/// Maximum lines for MEMORY.md.
const MAX_MEMORY_LINES: usize = 200;

pub struct MemoryCollector {
    cadence: CadenceGate,
    /// Hash of last injected content — skip if unchanged.
    last_content_hash: AtomicU64,
}

impl MemoryCollector {
    pub fn new(interval: usize) -> Self {
        Self {
            cadence: CadenceGate::new(interval),
            last_content_hash: AtomicU64::new(0),
        }
    }

    fn load_memory(working_dir: &std::path::Path) -> Option<String> {
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
}

#[async_trait::async_trait]
impl ContextCollector for MemoryCollector {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn should_fire(&self, ctx: &TurnContext<'_>) -> bool {
        self.cadence.should_fire(ctx.turn_number)
    }

    async fn collect(&self, ctx: &TurnContext<'_>) -> Option<Attachment> {
        let content = Self::load_memory(ctx.working_dir)?;
        let hash = Self::hash_content(&content);
        let prev = self.last_content_hash.load(Ordering::Relaxed);
        if hash == prev {
            return None; // unchanged since last injection
        }
        self.last_content_hash.store(hash, Ordering::Relaxed);

        Some(Attachment {
            name: "memory",
            content: format!("Project memory has been updated. Current MEMORY.md:\n\n{content}"),
            class: MessageClass::Nudge,
        })
    }

    fn did_fire(&self, turn: usize) {
        self.cadence.mark_fired(turn);
    }

    fn reset(&self) {
        self.cadence.reset();
        self.last_content_hash.store(0, Ordering::Relaxed);
    }
}
