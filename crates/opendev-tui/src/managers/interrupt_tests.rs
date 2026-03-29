use super::*;

#[test]
fn test_new_not_interrupted() {
    let mgr = InterruptManager::new();
    assert!(!mgr.is_interrupted());
}

#[test]
fn test_interrupt_and_clear() {
    let mgr = InterruptManager::new();
    mgr.interrupt();
    assert!(mgr.is_interrupted());

    mgr.clear();
    assert!(!mgr.is_interrupted());
}

#[test]
fn test_clone_shares_state() {
    let mgr = InterruptManager::new();
    let clone = mgr.clone();

    mgr.interrupt();
    assert!(clone.is_interrupted());

    clone.clear();
    assert!(!mgr.is_interrupted());
}

#[test]
fn test_interrupt_idempotent() {
    let mgr = InterruptManager::new();
    mgr.interrupt();
    mgr.interrupt();
    mgr.interrupt();
    assert!(mgr.is_interrupted());
}
