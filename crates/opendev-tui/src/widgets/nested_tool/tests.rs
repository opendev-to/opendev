use super::*;
use std::collections::HashMap;

#[test]
fn test_empty_widget() {
    let subagents: Vec<SubagentDisplayState> = vec![];
    let _widget = NestedToolWidget::new(&subagents);
}

#[test]
fn test_widget_with_active_subagent() {
    let mut state =
        SubagentDisplayState::new("id-1".into(), "Explore".into(), "Find TODOs".into());
    state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
    let subagents = vec![state];
    let _widget = NestedToolWidget::new(&subagents);
}

#[test]
fn test_widget_with_finished_subagent() {
    let mut state =
        SubagentDisplayState::new("id-2".into(), "Planner".into(), "Create plan".into());
    state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
    state.complete_tool_call("tc-1", true);
    state.add_tool_call("write_file".into(), "tc-2".into(), HashMap::new());
    state.complete_tool_call("tc-2", true);
    state.finish(true, "Plan created".into(), 2, None);
    let subagents = vec![state];
    let _widget = NestedToolWidget::new(&subagents);
}
