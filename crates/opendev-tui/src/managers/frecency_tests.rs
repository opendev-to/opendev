use super::*;

#[test]
fn test_new_empty() {
    let tracker = FrecencyTracker::new();
    assert!(tracker.is_empty());
    assert_eq!(tracker.score("anything"), 0.0);
}

#[test]
fn test_record_and_score() {
    let mut tracker = FrecencyTracker::new();
    tracker.record("hello");
    assert_eq!(tracker.len(), 1);

    // Just recorded, so hours_since ~= 0, score ~= frequency (1.0)
    let s = tracker.score("hello");
    assert!(s > 0.9 && s <= 1.0, "score was {}", s);
}

#[test]
fn test_multiple_records_increase_frequency() {
    let mut tracker = FrecencyTracker::new();
    tracker.record("cmd");
    tracker.record("cmd");
    tracker.record("cmd");

    // Frequency = 3, recency ~= 1.0, so score ~= 3.0
    let s = tracker.score("cmd");
    assert!(s > 2.9 && s <= 3.0, "score was {}", s);
}

#[test]
fn test_top_n() {
    let mut tracker = FrecencyTracker::new();
    tracker.record("rare");
    tracker.record("common");
    tracker.record("common");
    tracker.record("common");
    tracker.record("mid");
    tracker.record("mid");

    let top = tracker.top_n(2);
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].0, "common");
    assert_eq!(top[1].0, "mid");
}

#[test]
fn test_top_n_more_than_entries() {
    let mut tracker = FrecencyTracker::new();
    tracker.record("only");
    let top = tracker.top_n(10);
    assert_eq!(top.len(), 1);
}

#[test]
fn test_clear() {
    let mut tracker = FrecencyTracker::new();
    tracker.record("a");
    tracker.record("b");
    tracker.clear();
    assert!(tracker.is_empty());
}

#[test]
fn test_get_entry() {
    let mut tracker = FrecencyTracker::new();
    tracker.record("x");
    tracker.record("x");
    let entry = tracker.get("x").unwrap();
    assert_eq!(entry.frequency, 2);
}

#[test]
fn test_unrecorded_score_zero() {
    let tracker = FrecencyTracker::new();
    assert_eq!(tracker.score("nonexistent"), 0.0);
}
