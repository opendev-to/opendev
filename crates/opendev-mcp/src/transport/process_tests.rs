use super::*;

#[cfg(unix)]
#[test]
fn test_collect_descendant_pids_self_process() {
    // Our own PID should have no children in this test context.
    let my_pid = std::process::id();
    let descendants = collect_descendant_pids(my_pid);
    // We can't assert exact count (test runner may have threads),
    // but the function should not panic or hang.
    assert!(
        descendants.len() < 100,
        "Unreasonable number of descendants"
    );
}

#[cfg(unix)]
#[test]
fn test_collect_descendant_pids_nonexistent() {
    // A PID that almost certainly doesn't exist.
    let descendants = collect_descendant_pids(999_999_999);
    assert!(descendants.is_empty());
}

#[cfg(unix)]
#[test]
fn test_kill_descendant_pids_empty() {
    // Should be a no-op, no panics.
    kill_descendant_pids(&[]);
}
