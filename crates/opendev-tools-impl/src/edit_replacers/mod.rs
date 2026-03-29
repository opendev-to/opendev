//! 9-pass fuzzy matching chain for the edit tool.
//!
//! LLMs frequently produce slightly different whitespace, indentation, or escaping
//! in `old_content`. This module implements a chain of increasingly flexible
//! matching strategies, tried in order until one succeeds.
//!
//! Pass order (strictest to most flexible):
//! 1. Simple — exact string match
//! 2. LineTrimmed — trim leading/trailing whitespace per line
//! 3. BlockAnchor — match by first/last lines as anchors, similarity for middle
//! 4. WhitespaceNormalized — collapse all whitespace to single space
//! 5. IndentationFlexible — strip indentation, match stripped content
//! 6. EscapeNormalized — normalize escape sequences
//! 7. TrimmedBoundary — trim first/last lines of old_content
//! 8. ContextAware — use surrounding context lines to locate position
//! 9. MultiOccurrence — trimmed line-by-line match as last resort

mod diff;
mod passes;

pub use diff::unified_diff;

use passes::*;

/// Result of a successful fuzzy match: the actual substring found in the original.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// The actual content from the original file that matched.
    pub actual: String,
    /// Which replacer pass found the match (for logging).
    pub pass_name: &'static str,
}

/// Normalize line endings to `\n`.
pub fn normalize_line_endings(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Run the 9-pass replacer chain. Returns the actual substring in `original`
/// that matches `old_content`, or `None` if no pass succeeds.
pub fn find_match(original: &str, old_content: &str) -> Option<MatchResult> {
    let original = normalize_line_endings(original);
    let old_content = normalize_line_endings(old_content);

    #[allow(clippy::type_complexity)]
    let passes: &[(&str, fn(&str, &str) -> Option<String>)] = &[
        ("simple", simple_find),
        ("line_trimmed", line_trimmed_find),
        ("block_anchor", block_anchor_find),
        ("whitespace_normalized", whitespace_normalized_find),
        ("indentation_flexible", indentation_flexible_find),
        ("escape_normalized", escape_normalized_find),
        ("trimmed_boundary", trimmed_boundary_find),
        ("context_aware", context_aware_find),
        ("multi_occurrence", multi_occurrence_find),
    ];

    for &(name, finder) in passes {
        if let Some(actual) = finder(&original, &old_content) {
            return Some(MatchResult {
                actual,
                pass_name: name,
            });
        }
    }

    None
}

/// Find line numbers (1-indexed) of all occurrences of `needle` in `haystack`.
pub fn find_occurrence_positions(haystack: &str, needle: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut search_pos = 0;
    while let Some(slice) = haystack.get(search_pos..) {
        if let Some(pos) = slice.find(needle) {
            let abs_pos = search_pos + pos;
            let line_num = haystack[..abs_pos].matches('\n').count() + 1;
            positions.push(line_num);
            search_pos = abs_pos + 1;
            // Snap to next valid UTF-8 char boundary
            while search_pos < haystack.len() && !haystack.is_char_boundary(search_pos) {
                search_pos += 1;
            }
        } else {
            break;
        }
    }
    positions
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests;
