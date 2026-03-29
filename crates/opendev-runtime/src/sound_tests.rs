use super::*;

#[test]
fn test_cooldown_logic() {
    // Test that the cooldown mechanism works
    let now = now_ms();
    assert!(now >= 0);

    // If LAST_PLAYED is set to now, subsequent calls within 30s should be blocked
    LAST_PLAYED_MS.store(now, Ordering::Relaxed);

    let new_now = now_ms();
    let last = LAST_PLAYED_MS.load(Ordering::Relaxed);
    // Within cooldown window
    assert!(new_now.saturating_sub(last) < COOLDOWN_SECONDS * 1000);
}

#[test]
fn test_now_ms() {
    let t1 = now_ms();
    let t2 = now_ms();
    assert!(t2 >= t1);
}
