use super::*;

fn temp_history() -> CommandHistory {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.json");
    CommandHistory::with_path(path)
}

#[test]
fn test_empty_history() {
    let hist = temp_history();
    assert!(hist.is_empty());
    assert_eq!(hist.len(), 0);
}

#[test]
fn test_record_and_navigate() {
    let mut hist = temp_history();
    hist.record("first command");
    hist.record("second command");

    assert_eq!(hist.len(), 2);

    // Navigate up: should get most recent first
    let text = hist.navigate_up("current").unwrap();
    assert_eq!(text, "second command");

    let text = hist.navigate_up("current").unwrap();
    assert_eq!(text, "first command");

    // At the end, should stay at oldest
    let text = hist.navigate_up("current").unwrap();
    assert_eq!(text, "first command");

    // Navigate down
    let text = hist.navigate_down().unwrap();
    assert_eq!(text, "second command");

    // Down again: back to saved input
    let text = hist.navigate_down().unwrap();
    assert_eq!(text, "current");
}

#[test]
fn test_navigate_empty() {
    let mut hist = temp_history();
    assert!(hist.navigate_up("hello").is_none());
    assert!(hist.navigate_down().is_none());
}

#[test]
fn test_record_updates_existing() {
    let mut hist = temp_history();
    hist.record("hello");
    hist.record("world");

    // Re-recording "hello" should update its timestamp/count (not duplicate)
    hist.record("hello");

    assert_eq!(hist.len(), 2);
    // Navigate to find both entries
    let first = hist.navigate_up("").unwrap().to_string();
    let second = hist.navigate_up("").unwrap().to_string();
    // Both entries should be present regardless of order
    let mut found = vec![first, second];
    found.sort();
    assert_eq!(found, vec!["hello", "world"]);
}

#[test]
fn test_record_trims_whitespace() {
    let mut hist = temp_history();
    hist.record("  trimmed  ");
    let text = hist.navigate_up("").unwrap();
    assert_eq!(text, "trimmed");
}

#[test]
fn test_record_ignores_empty() {
    let mut hist = temp_history();
    hist.record("");
    hist.record("   ");
    assert!(hist.is_empty());
}

#[test]
fn test_reset_navigation() {
    let mut hist = temp_history();
    hist.record("command");
    hist.navigate_up("input");
    assert!(hist.is_navigating());
    hist.reset_navigation();
    assert!(!hist.is_navigating());
}

#[test]
fn test_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.json");

    {
        let mut hist = CommandHistory::with_path(path.clone());
        hist.record("persistent command");
        hist.record("another one");
    }

    // Load from same file
    let mut hist = CommandHistory::with_path(path);
    assert_eq!(hist.len(), 2);
    let text = hist.navigate_up("").unwrap();
    assert_eq!(text, "another one");
}

#[test]
fn test_max_entries() {
    let mut hist = temp_history();
    for i in 0..600 {
        hist.record(&format!("command-{}", i));
    }
    assert_eq!(hist.len(), MAX_HISTORY_ENTRIES);
}
