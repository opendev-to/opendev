use super::*;

#[test]
fn test_exit_commands() {
    let cmds = BuiltinCommands::new();
    let mut state = ReplState::default();

    assert_eq!(cmds.dispatch("/exit", "", &mut state), CommandOutcome::Exit);
    assert_eq!(cmds.dispatch("/quit", "", &mut state), CommandOutcome::Exit);
}

#[test]
fn test_help_command() {
    let cmds = BuiltinCommands::new();
    let mut state = ReplState::default();

    assert_eq!(
        cmds.dispatch("/help", "", &mut state),
        CommandOutcome::Handled
    );
}

#[test]
fn test_mode_switch() {
    let cmds = BuiltinCommands::new();
    let mut state = ReplState::default();
    assert_eq!(state.mode, OperationMode::Normal);

    cmds.dispatch("/mode", "plan", &mut state);
    assert_eq!(state.mode, OperationMode::Plan);

    cmds.dispatch("/mode", "normal", &mut state);
    assert_eq!(state.mode, OperationMode::Normal);

    // Empty arg defaults to normal
    state.mode = OperationMode::Plan;
    cmds.dispatch("/mode", "", &mut state);
    assert_eq!(state.mode, OperationMode::Normal);
}

#[test]
fn test_unknown_command() {
    let cmds = BuiltinCommands::new();
    let mut state = ReplState::default();

    assert_eq!(
        cmds.dispatch("/foobar", "", &mut state),
        CommandOutcome::Unknown
    );
}

#[test]
fn test_clear_command() {
    let cmds = BuiltinCommands::new();
    let mut state = ReplState::default();

    assert_eq!(
        cmds.dispatch("/clear", "", &mut state),
        CommandOutcome::Handled
    );
}

#[test]
fn test_autonomy_command() {
    let cmds = BuiltinCommands::new();
    let mut state = ReplState::default();
    assert_eq!(state.autonomy_level, AutonomyLevel::SemiAuto);

    cmds.dispatch("/autonomy", "manual", &mut state);
    assert_eq!(state.autonomy_level, AutonomyLevel::Manual);

    cmds.dispatch("/autonomy", "auto", &mut state);
    assert_eq!(state.autonomy_level, AutonomyLevel::Auto);

    cmds.dispatch("/autonomy", "semi-auto", &mut state);
    assert_eq!(state.autonomy_level, AutonomyLevel::SemiAuto);

    // Invalid value should not change level
    cmds.dispatch("/autonomy", "garbage", &mut state);
    assert_eq!(state.autonomy_level, AutonomyLevel::SemiAuto);
}

#[test]
fn test_status_command() {
    let cmds = BuiltinCommands::new();
    let mut state = ReplState::default();

    assert_eq!(
        cmds.dispatch("/status", "", &mut state),
        CommandOutcome::Handled
    );
}

#[test]
fn test_models_command_dispatches() {
    let cmds = BuiltinCommands::new();
    let mut state = ReplState::default();
    assert_eq!(
        cmds.dispatch("/models", "", &mut state),
        CommandOutcome::Handled
    );
}

#[test]
fn test_model_picker_with_cache() {
    let cmds = BuiltinCommands::new();
    let tmp = tempfile::TempDir::new().unwrap();
    let providers_dir = tmp.path().join("providers");
    std::fs::create_dir_all(&providers_dir).unwrap();

    let provider_json = serde_json::json!({
        "id": "test-provider",
        "name": "Test Provider",
        "description": "A test provider",
        "api_key_env": "TEST_KEY",
        "api_base_url": "https://api.test.com",
        "models": {
            "model-a": {
                "id": "model-a",
                "name": "Model A",
                "provider": "Test Provider",
                "context_length": 128000,
                "capabilities": ["text", "vision"],
                "pricing": {"input": 3.0, "output": 15.0, "unit": "per 1M tokens"},
                "recommended": true
            },
            "model-b": {
                "id": "model-b",
                "name": "Model B",
                "provider": "Test Provider",
                "context_length": 4096,
                "capabilities": ["text"],
                "pricing": {"input": 0.5, "output": 1.0, "unit": "per 1M tokens"},
                "recommended": false
            }
        }
    });

    std::fs::write(
        providers_dir.join("test-provider.json"),
        serde_json::to_string_pretty(&provider_json).unwrap(),
    )
    .unwrap();

    let entries = cmds.handle_model_picker(Some(tmp.path()));
    assert_eq!(entries.len(), 2);
    // All entries should reference the test provider
    assert!(entries.iter().all(|(pid, _)| pid == "test-provider"));
}

#[test]
fn test_model_picker_empty_cache() {
    let cmds = BuiltinCommands::new();
    let tmp = tempfile::TempDir::new().unwrap();
    // Set OPENDEV_DISABLE_REMOTE_MODELS to prevent network access in test
    // SAFETY: This test is single-threaded and the env var is restored immediately.
    unsafe { std::env::set_var("OPENDEV_DISABLE_REMOTE_MODELS", "1") };
    let entries = cmds.handle_model_picker(Some(tmp.path()));
    unsafe { std::env::remove_var("OPENDEV_DISABLE_REMOTE_MODELS") };
    assert!(entries.is_empty());
}
