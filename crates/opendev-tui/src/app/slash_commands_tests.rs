use super::super::*;

/// Helper: assert the last two messages are a SlashCommand echo + CommandResult.
fn assert_command_result(app: &App, cmd_contains: &str, result_contains: &str) {
    let msgs = &app.state.messages;
    assert!(
        msgs.len() >= 2,
        "Expected at least 2 messages, got {}",
        msgs.len()
    );
    let echo = &msgs[msgs.len() - 2];
    let result = &msgs[msgs.len() - 1];
    assert_eq!(echo.role, DisplayRole::SlashCommand, "echo role mismatch");
    assert!(
        echo.content.contains(cmd_contains),
        "echo '{}' missing '{cmd_contains}'",
        echo.content
    );
    assert_eq!(
        result.role,
        DisplayRole::CommandResult,
        "result role mismatch"
    );
    assert!(
        result.content.contains(result_contains),
        "result '{}' missing '{result_contains}'",
        result.content
    );
}

#[test]
fn test_slash_mode_with_arg() {
    let mut app = App::new();
    assert_eq!(app.state.mode, OperationMode::Normal);
    app.execute_slash_command("/mode plan");
    assert_eq!(app.state.mode, OperationMode::Plan);
    assert_command_result(&app, "/mode plan", "Mode set to Plan");
    app.execute_slash_command("/mode normal");
    assert_eq!(app.state.mode, OperationMode::Normal);
    assert_command_result(&app, "/mode normal", "Mode set to Normal");
}

#[test]
fn test_slash_mode_bad_arg() {
    let mut app = App::new();
    app.execute_slash_command("/mode bogus");
    assert_eq!(app.state.mode, OperationMode::Normal);
    assert_command_result(&app, "/mode bogus", "Unknown mode");
}

#[test]
fn test_slash_mode_no_arg_toggles() {
    let mut app = App::new();
    app.execute_slash_command("/mode");
    assert_eq!(app.state.mode, OperationMode::Plan);
    assert_command_result(&app, "/mode", "Mode set to Plan");
    app.execute_slash_command("/mode");
    assert_eq!(app.state.mode, OperationMode::Normal);
    assert_command_result(&app, "/mode", "Mode set to Normal");
}

#[test]
fn test_slash_autonomy_with_arg() {
    let mut app = App::new();
    app.execute_slash_command("/autonomy auto");
    assert_eq!(app.state.autonomy, AutonomyLevel::Auto);
    assert_command_result(&app, "/autonomy auto", "Autonomy set to Auto");
    app.execute_slash_command("/autonomy manual");
    assert_eq!(app.state.autonomy, AutonomyLevel::Manual);
    assert_command_result(&app, "/autonomy manual", "Autonomy set to Manual");
    app.execute_slash_command("/autonomy semi-auto");
    assert_eq!(app.state.autonomy, AutonomyLevel::SemiAuto);
    assert_command_result(&app, "/autonomy semi-auto", "Autonomy set to Semi");
}

#[test]
fn test_slash_autonomy_bad_arg() {
    let mut app = App::new();
    app.execute_slash_command("/autonomy bogus");
    assert_eq!(app.state.autonomy, AutonomyLevel::SemiAuto);
    assert_command_result(&app, "/autonomy bogus", "Unknown autonomy");
}

#[test]
fn test_slash_models_opens_picker_or_shows_message() {
    let mut app = App::new();
    app.execute_slash_command("/models");
    let has_picker = app.model_picker_controller.is_some();
    let has_message = app
        .state
        .messages
        .last()
        .is_some_and(|m| m.content.contains("No models"));
    assert!(
        has_picker || has_message,
        "Expected model picker or 'No models' message"
    );
}

#[test]
fn test_slash_tasks_empty() {
    let mut app = App::new();
    app.execute_slash_command("/tasks");
    assert_command_result(&app, "/tasks", "No background tasks");
}

#[test]
fn test_slash_task_no_arg() {
    let mut app = App::new();
    app.execute_slash_command("/task");
    assert_command_result(&app, "/task", "Usage");
}

#[test]
fn test_slash_kill_no_arg() {
    let mut app = App::new();
    app.execute_slash_command("/kill");
    assert_command_result(&app, "/kill", "Usage");
}

#[test]
fn test_slash_mcp_list_empty() {
    let mut app = App::new();
    app.execute_slash_command("/mcp list");
    assert_command_result(&app, "/mcp list", "No MCP servers");
}

#[test]
fn test_slash_init() {
    let mut app = App::new();
    app.execute_slash_command("/init");
    assert_command_result(&app, "/init", "Generating AGENTS.md");
}

#[test]
fn test_slash_agents() {
    let mut app = App::new();
    app.execute_slash_command("/agents");
    assert_command_result(&app, "/agents", "No custom agents");
}

#[test]
fn test_slash_skills() {
    let mut app = App::new();
    app.execute_slash_command("/skills");
    assert_command_result(&app, "/skills", "No custom skills");
}

#[test]
fn test_slash_plugins() {
    let mut app = App::new();
    app.execute_slash_command("/plugins");
    assert_command_result(&app, "/plugins", "No plugins");
}

#[test]
fn test_slash_help_lists_all_commands() {
    let mut app = App::new();
    app.execute_slash_command("/help");
    let result = &app.state.messages.last().unwrap().content;
    assert_eq!(
        app.state.messages.last().unwrap().role,
        DisplayRole::CommandResult
    );
    for cmd in &[
        "mode", "autonomy", "models", "mcp", "tasks", "task", "kill", "agents", "skills",
        "plugins", "undo", "redo", "share", "sessions",
    ] {
        assert!(result.contains(cmd), "Help text missing /{cmd}");
    }
}
