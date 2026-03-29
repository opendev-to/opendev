use super::*;
use crate::event::AppEvent;

#[test]
fn test_app_creation() {
    let app = App::new();
    assert!(app.state.running);
    assert_eq!(app.state.mode, OperationMode::Normal);
}

#[test]
fn test_should_render_before_draining_on_live_subagent_events() {
    assert!(App::should_render_before_draining(
        &AppEvent::ReasoningContent("thinking".into(),)
    ));
    assert!(App::should_render_before_draining(&AppEvent::ToolStarted {
        tool_id: "t1".into(),
        tool_name: "spawn_subagent".into(),
        args: std::collections::HashMap::new(),
    }));
    assert!(App::should_render_before_draining(
        &AppEvent::SubagentStarted {
            subagent_id: "sa1".into(),
            subagent_name: "Explore".into(),
            task: "Inspect auth".into(),
            cancel_token: None,
        }
    ));
    assert!(App::should_render_before_draining(
        &AppEvent::ToolFinished {
            tool_id: "t1".into(),
            success: true,
        }
    ));
}

#[test]
fn test_should_not_force_render_before_draining_on_tick() {
    assert!(!App::should_render_before_draining(&AppEvent::Tick));
}
