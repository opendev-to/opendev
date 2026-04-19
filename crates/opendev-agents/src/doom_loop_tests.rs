use super::*;

fn make_tool_call(name: &str, args: &str) -> Value {
    serde_json::json!({
        "id": "tc-1",
        "function": {"name": name, "arguments": args}
    })
}

#[test]
fn test_no_doom_loop_varied_calls() {
    let mut det = DoomLoopDetector::new();
    for i in 0..10 {
        let tc = make_tool_call("read_file", &format!("{{\"path\": \"file{i}.rs\"}}"));
        let (action, _) = det.check(&[tc]);
        assert_eq!(action, DoomLoopAction::None);
    }
}

#[test]
fn test_single_step_doom_loop() {
    let mut det = DoomLoopDetector::new();
    let tc = make_tool_call("read_file", "{\"path\": \"same.rs\"}");

    // First two calls: no doom loop
    let (action, _) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::None);
    let (action, _) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::None);

    // Third identical call: doom loop detected (Redirect)
    let (action, warning) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::Redirect);
    assert!(warning.contains("read_file"));
    assert!(warning.contains("3 times"));

    // Fourth identical call: Notify
    let (action, _) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::Notify);

    // Fifth: ForceStop
    let (action, _) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::ForceStop);
    assert_eq!(det.nudge_count(), 3);
}

#[test]
fn test_two_step_cycle() {
    let mut det = DoomLoopDetector::new();
    let edit = make_tool_call(
        "edit_file",
        "{\"path\": \"a.rs\", \"old\": \"x\", \"new\": \"y\"}",
    );
    let test = make_tool_call("bash", "{\"command\": \"cargo test\"}");

    // Need 2*3=6 calls to detect a 2-step cycle with threshold 3
    for _ in 0..2 {
        let (action, _) = det.check(&[edit.clone()]);
        assert_eq!(action, DoomLoopAction::None);
        let (action, _) = det.check(&[test.clone()]);
        assert_eq!(action, DoomLoopAction::None);
    }
    // 5th call (3rd edit)
    let (action, _) = det.check(&[edit.clone()]);
    assert_eq!(action, DoomLoopAction::None);
    // 6th call (3rd test) — completes 3 repetitions of the 2-step cycle
    let (action, warning) = det.check(&[test.clone()]);
    assert_eq!(action, DoomLoopAction::Redirect);
    assert!(warning.contains("2-step cycle"));
}

#[test]
fn test_reset() {
    let mut det = DoomLoopDetector::new();
    let tc = make_tool_call("read_file", "{\"path\": \"same.rs\"}");
    det.check(&[tc.clone()]);
    det.check(&[tc.clone()]);
    det.check(&[tc.clone()]);
    assert_eq!(det.nudge_count(), 1);

    det.reset();
    assert_eq!(det.nudge_count(), 0);

    // After reset, no doom loop since history is cleared
    let (action, _) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::None);
}

#[test]
fn test_fingerprint_deterministic() {
    let fp1 = DoomLoopDetector::fingerprint("read_file", "{\"path\": \"a.rs\"}");
    let fp2 = DoomLoopDetector::fingerprint("read_file", "{\"path\": \"a.rs\"}");
    assert_eq!(fp1, fp2);

    let fp3 = DoomLoopDetector::fingerprint("read_file", "{\"path\": \"b.rs\"}");
    assert_ne!(fp1, fp3);
}

#[test]
fn test_parallel_tool_calls_redirect() {
    let mut det = DoomLoopDetector::new();
    let tc = make_tool_call("search", "{\"query\": \"foo\"}");

    // Submit 3 identical calls in one batch
    let (action, _) = det.check(&[tc.clone(), tc.clone(), tc.clone()]);
    assert_eq!(action, DoomLoopAction::Redirect);
}

#[test]
fn test_three_step_cycle() {
    let mut det = DoomLoopDetector::new();
    let a = make_tool_call("read_file", "{\"path\": \"a\"}");
    let b = make_tool_call("edit_file", "{\"path\": \"b\"}");
    let c = make_tool_call("bash", "{\"cmd\": \"test\"}");

    // 3*3=9 calls for a 3-step cycle
    for round in 0..3 {
        let (action, _) = det.check(&[a.clone()]);
        if round < 2 {
            assert_eq!(action, DoomLoopAction::None);
        }
        let (action, _) = det.check(&[b.clone()]);
        if round < 2 {
            assert_eq!(action, DoomLoopAction::None);
        }
        let (action, warning) = det.check(&[c.clone()]);
        if round < 2 {
            assert_eq!(action, DoomLoopAction::None);
        } else {
            assert_eq!(action, DoomLoopAction::Redirect);
            assert!(warning.contains("3-step cycle"));
        }
    }
}

#[test]
fn test_recovery_action_redirect_returns_nudge() {
    let mut det = DoomLoopDetector::new();
    let tc = make_tool_call("read_file", "{\"path\": \"same.rs\"}");

    // Trigger first detection (Redirect)
    det.check(&[tc.clone()]);
    det.check(&[tc.clone()]);
    let (action, _warning) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::Redirect);

    let recovery = det.recovery_action(&action);
    match recovery {
        RecoveryAction::Nudge(msg) => {
            assert!(msg.contains("STOP and try something different"));
        }
        other => panic!("Expected Nudge, got {:?}", other),
    }
}

#[test]
fn test_recovery_action_notify_returns_step_back() {
    let mut det = DoomLoopDetector::new();
    let tc = make_tool_call("read_file", "{\"path\": \"same.rs\"}");

    // First detection (Redirect)
    det.check(&[tc.clone()]);
    det.check(&[tc.clone()]);
    det.check(&[tc.clone()]);

    // Second detection (Notify)
    let (action, _warning) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::Notify);

    let recovery = det.recovery_action(&action);
    match recovery {
        RecoveryAction::StepBack(msg) => {
            assert!(msg.contains("stuck in a loop"));
        }
        other => panic!("Expected StepBack, got {:?}", other),
    }
}

#[test]
fn test_recovery_action_force_stop_returns_compact() {
    let det = DoomLoopDetector::new();
    let recovery = det.recovery_action(&DoomLoopAction::ForceStop);
    assert_eq!(recovery, RecoveryAction::CompactContext);
}

#[test]
fn test_recovery_action_none_returns_empty_nudge() {
    let det = DoomLoopDetector::new();
    let recovery = det.recovery_action(&DoomLoopAction::None);
    match recovery {
        RecoveryAction::Nudge(msg) => assert!(msg.is_empty()),
        other => panic!("Expected empty Nudge, got {:?}", other),
    }
}
