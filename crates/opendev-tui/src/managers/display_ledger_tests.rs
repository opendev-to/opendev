use super::*;

#[test]
fn test_new_is_empty() {
    let ledger = DisplayLedger::new();
    assert!(ledger.is_empty());
    assert_eq!(ledger.len(), 0);
}

#[test]
fn test_mark_and_check() {
    let mut ledger = DisplayLedger::new();
    assert!(!ledger.is_rendered("msg-1"));

    assert!(ledger.mark_rendered("msg-1")); // first time -> true
    assert!(ledger.is_rendered("msg-1"));
    assert_eq!(ledger.len(), 1);

    assert!(!ledger.mark_rendered("msg-1")); // duplicate -> false
    assert_eq!(ledger.len(), 1);
}

#[test]
fn test_clear() {
    let mut ledger = DisplayLedger::new();
    ledger.mark_rendered("a");
    ledger.mark_rendered("b");
    assert_eq!(ledger.len(), 2);

    ledger.clear();
    assert!(ledger.is_empty());
    assert!(!ledger.is_rendered("a"));
}
