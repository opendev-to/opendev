use super::*;

#[test]
fn test_cadence_gate_fires_at_interval() {
    let gate = CadenceGate::new(5);
    // Turn 0: last_fired=0, 0-0=0 < 5 → false
    assert!(!gate.should_fire(0));
    assert!(!gate.should_fire(4));
    // Turn 5: 5-0=5 >= 5 → true
    assert!(gate.should_fire(5));

    gate.mark_fired(5);
    // Turn 5 after fire: 5-5=0 < 5 → false
    assert!(!gate.should_fire(5));
    assert!(!gate.should_fire(9));
    // Turn 10: 10-5=5 >= 5 → true
    assert!(gate.should_fire(10));
}

#[test]
fn test_cadence_gate_reset() {
    let gate = CadenceGate::new(10);
    gate.mark_fired(15);
    assert!(!gate.should_fire(20));

    gate.reset();
    // After reset, last_fired=0, so 20-0=20 >= 10 → fires
    assert!(gate.should_fire(20));
}

#[test]
fn test_collector_runner_empty() {
    let runner = CollectorRunner::new(vec![]);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut messages = vec![];
    let ctx = TurnContext {
        turn_number: 1,
        working_dir: std::path::Path::new("/tmp"),
        todo_manager: None,
        shared_state: None,
        last_user_query: None,
    };
    rt.block_on(runner.run(&ctx, &mut messages));
    assert!(messages.is_empty());
}
