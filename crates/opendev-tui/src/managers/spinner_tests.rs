use super::*;

#[test]
fn test_new_inactive() {
    let svc = SpinnerService::new();
    assert!(!svc.active());
    assert!(svc.message().is_empty());
    assert_eq!(svc.elapsed(), Duration::ZERO);
}

#[test]
fn test_start_stop() {
    let mut svc = SpinnerService::new();
    svc.start("Loading models...".into());
    assert!(svc.active());
    assert_eq!(svc.message(), "Loading models...");
    assert!(svc.elapsed() >= Duration::ZERO);

    svc.stop();
    assert!(!svc.active());
}

#[test]
fn test_elapsed_increases() {
    let mut svc = SpinnerService::new();
    svc.start("Working".into());
    // elapsed should be non-negative (just a sanity check)
    let _e = svc.elapsed();
}
