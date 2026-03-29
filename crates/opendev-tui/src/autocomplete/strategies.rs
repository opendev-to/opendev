//! Completion scoring and ranking strategies.
//!
//! Mirrors the Python `CompletionStrategy` — supports prefix matching, fuzzy
//! matching, and frecency-weighted ranking.

use std::collections::HashMap;
use std::time::Instant;

use super::CompletionItem;

// ── Frecency tracker ───────────────────────────────────────────────

/// Tracks access frequency and recency for a set of keys.
///
/// The score formula is `frequency * recency_weight` where `recency_weight`
/// decays over time.
#[derive(Debug)]
struct FrecencyEntry {
    /// Total number of accesses.
    count: u32,
    /// Timestamp of the last access.
    last_access: Instant,
}

/// Manages frecency data for completion items.
#[derive(Debug)]
pub struct FrecencyTracker {
    entries: HashMap<String, FrecencyEntry>,
}

impl FrecencyTracker {
    /// Create an empty tracker.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Record an access for `key`.
    pub fn record(&mut self, key: &str) {
        let entry = self
            .entries
            .entry(key.to_string())
            .or_insert(FrecencyEntry {
                count: 0,
                last_access: Instant::now(),
            });
        entry.count += 1;
        entry.last_access = Instant::now();
    }

    /// Compute a frecency score for `key`. Returns 0.0 if the key has never
    /// been accessed.
    pub fn score(&self, key: &str) -> f64 {
        match self.entries.get(key) {
            None => 0.0,
            Some(entry) => {
                let elapsed_secs = entry.last_access.elapsed().as_secs_f64();
                // Recency weight: 1.0 right after access, decaying with a
                // half-life of ~5 minutes.
                let recency = (-elapsed_secs / 300.0).exp();
                entry.count as f64 * recency
            }
        }
    }
}

impl Default for FrecencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Fuzzy matching ─────────────────────────────────────────────────

/// Simple fuzzy-match scoring.
///
/// Returns a score in `[0.0, 1.0]` where 1.0 is a perfect prefix match.
/// Returns 0.0 if the characters of `pattern` do not appear in order in
/// `text`.
pub fn fuzzy_score(pattern: &str, text: &str) -> f64 {
    if pattern.is_empty() {
        return 1.0;
    }
    let pattern_lower: Vec<char> = pattern.to_lowercase().chars().collect();
    let text_lower: Vec<char> = text.to_lowercase().chars().collect();

    let mut pi = 0; // index into pattern
    let mut consecutive = 0u32;
    let mut total_bonus = 0.0f64;
    let mut matched = false;

    for (ti, &tc) in text_lower.iter().enumerate() {
        if pi < pattern_lower.len() && tc == pattern_lower[pi] {
            // Bonus for matching at the start of the string or after a separator
            if ti == 0
                || matches!(
                    text_lower.get(ti.wrapping_sub(1)),
                    Some(&'/' | &'_' | &'-' | &'.')
                )
            {
                total_bonus += 0.15;
            }
            consecutive += 1;
            total_bonus += consecutive as f64 * 0.05;
            pi += 1;
        } else {
            consecutive = 0;
        }
    }

    if pi == pattern_lower.len() {
        matched = true;
    }

    if !matched {
        return 0.0;
    }

    // Base score: ratio of matched chars to text length
    let base = pattern_lower.len() as f64 / text_lower.len().max(1) as f64;
    (base + total_bonus).min(1.0)
}

// ── CompletionStrategy ─────────────────────────────────────────────

/// The matching mode used by [`CompletionStrategy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    /// Only prefix matches.
    Prefix,
    /// Fuzzy substring matching with scoring.
    Fuzzy,
}

/// Configurable strategy for scoring and sorting completion items.
pub struct CompletionStrategy {
    mode: MatchMode,
    frecency: FrecencyTracker,
    /// Weight applied to the frecency component (0.0 to disable).
    frecency_weight: f64,
}

impl CompletionStrategy {
    /// Create a strategy with the given mode.
    pub fn new(mode: MatchMode) -> Self {
        Self {
            mode,
            frecency: FrecencyTracker::new(),
            frecency_weight: 5.0,
        }
    }

    /// Record a frecency access.
    pub fn record_access(&mut self, key: &str) {
        self.frecency.record(key);
    }

    /// Sort `items` in-place, assigning scores and ordering by descending
    /// score.
    pub fn sort(&self, items: &mut [CompletionItem]) {
        for item in items.iter_mut() {
            let frecency = self.frecency.score(&item.insert_text) * self.frecency_weight;
            // For items already produced by a completer the base score is 0.
            // We can overlay a fuzzy score if in fuzzy mode.
            let match_score = match self.mode {
                MatchMode::Prefix => {
                    // Items are already prefix-filtered by the completer; give
                    // a small bonus for shorter labels (more relevant).
                    1.0 / (item.label.len() as f64 + 1.0)
                }
                MatchMode::Fuzzy => {
                    // Re-score using the label vs some implicit query. Since
                    // the completer already filtered, we just reward short
                    // labels.
                    1.0 / (item.label.len() as f64 + 1.0)
                }
            };
            item.score = match_score + frecency;
        }
        items.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Return the current match mode.
    pub fn mode(&self) -> MatchMode {
        self.mode
    }

    /// Set the match mode.
    pub fn set_mode(&mut self, mode: MatchMode) {
        self.mode = mode;
    }
}

impl Default for CompletionStrategy {
    fn default() -> Self {
        Self::new(MatchMode::Prefix)
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "strategies_tests.rs"]
mod tests;
