//! Display ledger for deduplicating rendered messages.
//!
//! Mirrors Python's `DisplayLedger` from
//! `opendev/ui_textual/managers/display_ledger.py`.
//!
//! Tracks which message IDs have been rendered to prevent duplicate display
//! across multiple code paths (streaming, history hydration, etc.).

use std::collections::HashSet;

/// Tracks which messages have been rendered to prevent duplicates.
pub struct DisplayLedger {
    rendered: HashSet<String>,
}

impl DisplayLedger {
    /// Create a new empty ledger.
    pub fn new() -> Self {
        Self {
            rendered: HashSet::new(),
        }
    }

    /// Mark a message ID as rendered.
    ///
    /// Returns `true` if this is the first time the ID was marked (i.e. not a duplicate).
    pub fn mark_rendered(&mut self, id: &str) -> bool {
        self.rendered.insert(id.to_string())
    }

    /// Check whether a message ID has been rendered.
    pub fn is_rendered(&self, id: &str) -> bool {
        self.rendered.contains(id)
    }

    /// Clear all rendered tracking state.
    pub fn clear(&mut self) {
        self.rendered.clear();
    }

    /// Number of tracked rendered messages.
    pub fn len(&self) -> usize {
        self.rendered.len()
    }

    /// Whether the ledger is empty.
    pub fn is_empty(&self) -> bool {
        self.rendered.is_empty()
    }
}

impl Default for DisplayLedger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "display_ledger_tests.rs"]
mod tests;
