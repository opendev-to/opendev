use super::*;
use crate::autocomplete::CompletionKind;

#[test]
fn test_fuzzy_score_exact() {
    let score = fuzzy_score("help", "help");
    assert!(score > 0.5, "exact match should score high: {}", score);
}

#[test]
fn test_fuzzy_score_prefix() {
    let score = fuzzy_score("hel", "help");
    assert!(score > 0.3, "prefix should score well: {}", score);
}

#[test]
fn test_fuzzy_score_no_match() {
    let score = fuzzy_score("xyz", "help");
    assert_eq!(score, 0.0);
}

#[test]
fn test_fuzzy_score_empty_pattern() {
    let score = fuzzy_score("", "anything");
    assert_eq!(score, 1.0);
}

#[test]
fn test_fuzzy_score_subsequence() {
    let score = fuzzy_score("hp", "help");
    assert!(score > 0.0, "subsequence should match: {}", score);
}

#[test]
fn test_frecency_new_key() {
    let tracker = FrecencyTracker::new();
    assert_eq!(tracker.score("unknown"), 0.0);
}

#[test]
fn test_frecency_after_access() {
    let mut tracker = FrecencyTracker::new();
    tracker.record("foo");
    let score = tracker.score("foo");
    assert!(score > 0.0);
}

#[test]
fn test_frecency_multiple_accesses() {
    let mut tracker = FrecencyTracker::new();
    tracker.record("foo");
    tracker.record("foo");
    tracker.record("foo");
    let s3 = tracker.score("foo");
    // Three accesses should score higher than one
    let mut tracker2 = FrecencyTracker::new();
    tracker2.record("foo");
    let s1 = tracker2.score("foo");
    assert!(s3 > s1);
}

#[test]
fn test_strategy_sort_by_label_length() {
    let strategy = CompletionStrategy::default();
    let mut items = vec![
        CompletionItem {
            insert_text: "/session-models".into(),
            label: "/session-models".into(),
            description: String::new(),
            kind: CompletionKind::Command,
            score: 0.0,
        },
        CompletionItem {
            insert_text: "/help".into(),
            label: "/help".into(),
            description: String::new(),
            kind: CompletionKind::Command,
            score: 0.0,
        },
    ];
    strategy.sort(&mut items);
    // Shorter label ("/help") should rank first
    assert_eq!(items[0].label, "/help");
}

#[test]
fn test_strategy_frecency_boost() {
    let mut strategy = CompletionStrategy::default();
    strategy.record_access("/exit");

    let mut items = vec![
        CompletionItem {
            insert_text: "/help".into(),
            label: "/help".into(),
            description: String::new(),
            kind: CompletionKind::Command,
            score: 0.0,
        },
        CompletionItem {
            insert_text: "/exit".into(),
            label: "/exit".into(),
            description: String::new(),
            kind: CompletionKind::Command,
            score: 0.0,
        },
    ];
    strategy.sort(&mut items);
    // "/exit" has frecency boost and same length as "/help", should rank first
    assert_eq!(items[0].label, "/exit");
}
