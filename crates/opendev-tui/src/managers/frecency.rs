//! Frecency-based scoring for suggestion ranking.
//!
//! Mirrors Python's `FrecencyManager` from
//! `opendev/ui_textual/managers/frecency_manager.py`.
//!
//! Score formula: `frequency * (1.0 / (1.0 + hours_since_last_use))`

use std::collections::HashMap;
use std::time::Instant;

/// Entry tracking usage frequency and recency.
#[derive(Debug, Clone)]
pub struct FrecencyEntry {
    /// Number of times this item has been used.
    pub frequency: u64,
    /// When the item was last used.
    pub last_used: Instant,
}

/// Tracks and scores items by frequency and recency.
pub struct FrecencyTracker {
    entries: HashMap<String, FrecencyEntry>,
}

impl FrecencyTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Record a usage of the given key.
    pub fn record(&mut self, key: &str) {
        let now = Instant::now();
        self.entries
            .entry(key.to_string())
            .and_modify(|e| {
                e.frequency += 1;
                e.last_used = now;
            })
            .or_insert(FrecencyEntry {
                frequency: 1,
                last_used: now,
            });
    }

    /// Calculate the frecency score for a key.
    ///
    /// Returns 0.0 if the key has never been recorded.
    /// Score = frequency * (1.0 / (1.0 + hours_since_last_use))
    pub fn score(&self, key: &str) -> f64 {
        match self.entries.get(key) {
            Some(entry) => {
                let hours = entry.last_used.elapsed().as_secs_f64() / 3600.0;
                entry.frequency as f64 * (1.0 / (1.0 + hours))
            }
            None => 0.0,
        }
    }

    /// Get the top N items sorted by frecency score (highest first).
    pub fn top_n(&self, n: usize) -> Vec<(&str, f64)> {
        let mut scored: Vec<(&str, f64)> = self
            .entries
            .keys()
            .map(|k| (k.as_str(), self.score(k)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(n);
        scored
    }

    /// Get the entry for a key, if it exists.
    pub fn get(&self, key: &str) -> Option<&FrecencyEntry> {
        self.entries.get(key)
    }

    /// Number of tracked items.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the tracker is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for FrecencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "frecency_tests.rs"]
mod tests;
