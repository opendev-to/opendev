use super::*;

#[test]
fn test_operation_mode_display() {
    assert_eq!(OperationMode::Normal.to_string(), "NORMAL");
    assert_eq!(OperationMode::Plan.to_string(), "PLAN");
}

#[test]
fn test_repl_state_default() {
    let state = ReplState::default();
    assert_eq!(state.mode, OperationMode::Normal);
    assert_eq!(state.autonomy_level, AutonomyLevel::SemiAuto);
    assert!(state.running);
    assert!(state.last_prompt.is_empty());
    assert_eq!(state.last_operation_summary, "—");
    assert!(state.last_error.is_none());
    assert!(state.last_latency_ms.is_none());
    assert!(!state.pending_plan_request);
}

#[test]
fn test_autonomy_level_in_state() {
    let mut state = ReplState::default();
    state.autonomy_level = AutonomyLevel::Auto;
    assert_eq!(state.autonomy_level, AutonomyLevel::Auto);
}
