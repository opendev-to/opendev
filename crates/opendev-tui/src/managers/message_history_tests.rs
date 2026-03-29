use super::*;

#[test]
fn test_new_empty() {
    let hist = MessageHistory::new(100);
    assert!(hist.is_empty());
    assert_eq!(hist.len(), 0);
}

#[test]
fn test_push_and_navigate() {
    let mut hist = MessageHistory::new(100);
    hist.push("first".into());
    hist.push("second".into());
    hist.push("third".into());
    assert_eq!(hist.len(), 3);

    // up() returns most recent first
    assert_eq!(hist.up(), Some("third"));
    assert_eq!(hist.up(), Some("second"));
    assert_eq!(hist.up(), Some("first"));
    assert_eq!(hist.up(), Some("first")); // stays at oldest

    // down() navigates back
    assert_eq!(hist.down(), Some("second"));
    assert_eq!(hist.down(), Some("third"));
    assert_eq!(hist.down(), None); // past newest
}

#[test]
fn test_capacity_eviction() {
    let mut hist = MessageHistory::new(3);
    hist.push("a".into());
    hist.push("b".into());
    hist.push("c".into());
    hist.push("d".into()); // evicts "a"
    assert_eq!(hist.len(), 3);

    assert_eq!(hist.up(), Some("d"));
    assert_eq!(hist.up(), Some("c"));
    assert_eq!(hist.up(), Some("b"));
    assert_eq!(hist.up(), Some("b")); // "a" is gone
}

#[test]
fn test_empty_push_ignored() {
    let mut hist = MessageHistory::new(100);
    hist.push("".into());
    assert!(hist.is_empty());
}

#[test]
fn test_consecutive_duplicate_ignored() {
    let mut hist = MessageHistory::new(100);
    hist.push("same".into());
    hist.push("same".into());
    assert_eq!(hist.len(), 1);
}

#[test]
fn test_up_empty() {
    let mut hist = MessageHistory::new(100);
    assert_eq!(hist.up(), None);
}

#[test]
fn test_down_without_navigating() {
    let mut hist = MessageHistory::new(100);
    hist.push("msg".into());
    assert_eq!(hist.down(), None);
}

#[test]
fn test_reset_cursor() {
    let mut hist = MessageHistory::new(100);
    hist.push("a".into());
    hist.push("b".into());
    hist.up();
    hist.reset_cursor();
    // After reset, up() should start from newest again
    assert_eq!(hist.up(), Some("b"));
}
