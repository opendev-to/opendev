use super::*;

#[test]
fn test_new_is_inactive() {
    let ctrl = SpinnerController::new();
    assert!(!ctrl.active());
    assert!(ctrl.message().is_empty());
}

#[test]
fn test_start_stop() {
    let mut ctrl = SpinnerController::new();
    ctrl.start("Loading...".into());
    assert!(ctrl.active());
    assert_eq!(ctrl.message(), "Loading...");

    ctrl.stop();
    assert!(!ctrl.active());
}

#[test]
fn test_tick_cycles() {
    let mut ctrl = SpinnerController::new();
    ctrl.start("Working".into());

    let first = ctrl.tick();
    assert_eq!(first, "⠋");

    let second = ctrl.tick();
    assert_eq!(second, "⠙");

    // Cycle through all frames
    for _ in 0..8 {
        ctrl.tick();
    }
    // Should wrap back to first frame
    let wrapped = ctrl.tick();
    assert_eq!(wrapped, "⠋");
}

#[test]
fn test_frames_count() {
    assert_eq!(SpinnerController::frames().len(), 10);
}
