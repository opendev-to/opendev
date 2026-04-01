//! Per-turn context attachment system.
//!
//! Pluggable collectors gather live runtime state (todos, git, plan mode, etc.)
//! before each LLM call and inject it as `<system-reminder>` messages via the
//! existing `inject_system_message()` infrastructure.
//!
//! This complements the reactive nudge system (tool failures, doom loops) and
//! the `ProactiveReminderScheduler` (static template reminders) by providing
//! **live data** — actual todo items, current git branch, etc.

pub mod collectors;

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::prompts::reminders::{MessageClass, inject_system_message};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Immutable snapshot of runtime state available to collectors each turn.
pub struct TurnContext<'a> {
    pub turn_number: usize,
    pub working_dir: &'a std::path::Path,
    pub todo_manager: Option<&'a std::sync::Mutex<opendev_runtime::TodoManager>>,
    pub shared_state:
        Option<&'a std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>>,
    /// The user's most recent query text, used for semantic memory selection.
    pub last_user_query: Option<&'a str>,
}

/// Output produced when a collector fires.
pub struct Attachment {
    pub name: &'static str,
    pub content: String,
    pub class: MessageClass,
}

// ---------------------------------------------------------------------------
// Collector trait
// ---------------------------------------------------------------------------

/// A collector that gathers live data from one source per turn.
///
/// Each collector is responsible for:
/// - Deciding whether to fire this turn (`should_fire`)
/// - Producing rendered content from live data (`collect`)
/// - Tracking its own cadence state via interior mutability
///
/// Implementations must be `Send + Sync` (shared across async boundaries).
/// Use `CadenceGate` or atomics for mutable cadence state since the trait
/// uses `&self`.
#[async_trait::async_trait]
pub trait ContextCollector: Send + Sync {
    /// Human-readable name for logging/debugging.
    fn name(&self) -> &'static str;

    /// Fast, synchronous gate. Return `false` to skip `collect()` entirely.
    fn should_fire(&self, ctx: &TurnContext<'_>) -> bool;

    /// Produce the attachment content from live data.
    /// Returns `None` if there is nothing meaningful to inject this turn.
    async fn collect(&self, ctx: &TurnContext<'_>) -> Option<Attachment>;

    /// Called after a successful fire. Update cadence state here.
    fn did_fire(&self, _turn: usize) {}

    /// Reset internal state (e.g., after context compaction).
    fn reset(&self) {}
}

// ---------------------------------------------------------------------------
// Cadence gate (reusable frequency control)
// ---------------------------------------------------------------------------

/// Turn-count-based gate for controlling how often a collector fires.
///
/// Uses atomics for interior mutability (required because `ContextCollector`
/// methods take `&self`).
pub struct CadenceGate {
    interval: usize,
    last_fired: AtomicUsize,
}

impl CadenceGate {
    /// Create a gate that fires every `interval` turns.
    pub fn new(interval: usize) -> Self {
        Self {
            interval,
            last_fired: AtomicUsize::new(0),
        }
    }

    /// Check if enough turns have passed since the last fire.
    pub fn should_fire(&self, turn: usize) -> bool {
        let last = self.last_fired.load(Ordering::Relaxed);
        turn.saturating_sub(last) >= self.interval
    }

    /// Record that the gate fired at the given turn.
    pub fn mark_fired(&self, turn: usize) {
        self.last_fired.store(turn, Ordering::Relaxed);
    }

    /// Reset to allow immediate firing on next check.
    pub fn reset(&self) {
        self.last_fired.store(0, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Collector runner (orchestrator)
// ---------------------------------------------------------------------------

/// Runs all registered collectors and injects produced attachments.
pub struct CollectorRunner {
    collectors: Vec<Box<dyn ContextCollector>>,
}

impl CollectorRunner {
    pub fn new(collectors: Vec<Box<dyn ContextCollector>>) -> Self {
        Self { collectors }
    }

    /// Run all collectors for this turn, injecting attachments into messages.
    pub async fn run(&self, ctx: &TurnContext<'_>, messages: &mut Vec<serde_json::Value>) {
        for collector in &self.collectors {
            if collector.should_fire(ctx)
                && let Some(attachment) = collector.collect(ctx).await
            {
                tracing::debug!(
                    collector = collector.name(),
                    attachment = attachment.name,
                    "Injecting context attachment"
                );
                inject_system_message(messages, &attachment.content, attachment.class);
                collector.did_fire(ctx.turn_number);
            }
        }
    }

    /// Reset all collectors (e.g., after context compaction).
    pub fn reset_all(&self) {
        for collector in &self.collectors {
            collector.reset();
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
