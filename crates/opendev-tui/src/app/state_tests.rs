use super::*;

#[test]
fn test_app_state_default() {
    let state = AppState::default();
    assert!(state.running);
    assert_eq!(state.mode, OperationMode::Normal);
    assert!(state.messages.is_empty());
    assert!(state.input_buffer.is_empty());
}

#[test]
fn test_dirty_flag_default() {
    let state = AppState::default();
    assert!(
        state.dirty,
        "AppState should start dirty for initial render"
    );
}
