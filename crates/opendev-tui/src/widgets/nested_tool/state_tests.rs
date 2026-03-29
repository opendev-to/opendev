use super::*;

#[test]
fn test_subagent_display_state_new() {
    let state = SubagentDisplayState::new("id-1".into(), "Explore".into(), "Find TODOs".into());
    assert_eq!(state.name, "Explore");
    assert!(!state.finished);
    assert_eq!(state.tool_call_count, 0);
}

#[test]
fn test_add_and_complete_tool_call() {
    let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
    state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
    assert_eq!(state.tool_call_count, 1);
    assert!(state.active_tools.contains_key("tc-1"));

    state.complete_tool_call("tc-1", true);
    assert!(state.active_tools.is_empty());
    assert_eq!(state.completed_tools.len(), 1);
    assert!(state.completed_tools[0].success);
}

#[test]
fn test_finish() {
    let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
    state.finish(true, "Done".into(), 3, None);
    assert!(state.finished);
    assert!(state.success);
    assert_eq!(state.result_summary, "Done");
    assert_eq!(state.tool_call_count, 3);
}

#[test]
fn test_finish_preserves_higher_live_count() {
    let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
    // Simulate 10 live tool calls
    for i in 0..10 {
        let id = format!("tc-{i}");
        state.add_tool_call("read_file".into(), id.clone(), HashMap::new());
        state.complete_tool_call(&id, true);
    }
    assert_eq!(state.tool_call_count, 10);
    // finish() with a lower message-based count should NOT decrease the count
    state.finish(true, "Done".into(), 4, None);
    assert_eq!(state.tool_call_count, 10);
}

#[test]
fn test_finish_with_shallow_warning() {
    let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
    state.finish(
        true,
        "Done".into(),
        1,
        Some("Shallow subagent warning".into()),
    );
    assert!(state.shallow_warning.is_some());
}

#[test]
fn test_advance_tick() {
    let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
    state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
    state.advance_tick();
    assert_eq!(state.tick, 1);
    assert_eq!(state.active_tools["tc-1"].tick, 1);
}

#[test]
fn test_token_accumulation() {
    let mut state = SubagentDisplayState::new("id-tok".into(), "test".into(), "task".into());
    assert_eq!(state.token_count, 0);
    state.add_tokens(1000, 500);
    assert_eq!(state.token_count, 1500);
    state.add_tokens(2000, 300);
    assert_eq!(state.token_count, 3800);
}

#[test]
fn test_completed_tools_cap() {
    let mut state = SubagentDisplayState::new("id-cap".into(), "test".into(), "task".into());
    // Add 150 tool calls and complete them all
    for i in 0..150 {
        let id = format!("tc-{i}");
        state.add_tool_call("read_file".into(), id.clone(), HashMap::new());
        state.complete_tool_call(&id, true);
    }
    // Should be capped at 100
    assert_eq!(state.completed_tools.len(), 100);
    assert_eq!(state.tool_call_count, 150);
}

#[test]
fn test_activity_summary_reading() {
    let mut state = SubagentDisplayState::new("id-act".into(), "test".into(), "task".into());
    state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
    state.add_tool_call("read_file".into(), "tc-2".into(), HashMap::new());
    state.add_tool_call("read_file".into(), "tc-3".into(), HashMap::new());
    assert_eq!(state.activity_summary(), "Reading 3 files...");
}

#[test]
fn test_activity_summary_searching() {
    let mut state = SubagentDisplayState::new("id-act2".into(), "test".into(), "task".into());
    state.add_tool_call("grep".into(), "tc-1".into(), HashMap::new());
    state.add_tool_call("list_files".into(), "tc-2".into(), HashMap::new());
    assert_eq!(state.activity_summary(), "Searching for 2 patterns...");
}

#[test]
fn test_activity_summary_running() {
    let state = SubagentDisplayState::new("id-act3".into(), "test".into(), "task".into());
    assert_eq!(state.activity_summary(), "Running...");
}

#[test]
fn test_activity_summary_done() {
    let mut state = SubagentDisplayState::new("id-act4".into(), "test".into(), "task".into());
    state.finish(true, "Done".into(), 0, None);
    assert_eq!(state.activity_summary(), "Done");
}

#[test]
fn test_format_token_count() {
    assert_eq!(format_token_count(500), "500 tokens");
    assert_eq!(format_token_count(1_500), "1.5k tokens");
    assert_eq!(format_token_count(23_456), "23.5k tokens");
    assert_eq!(format_token_count(1_500_000), "1.5M tokens");
}

#[test]
fn test_completion_summary_with_tokens() {
    let mut state = SubagentDisplayState::new("id-cs".into(), "Explore".into(), "task".into());
    // Simulate tool calls and tokens
    for i in 0..5 {
        let id = format!("tc-{i}");
        state.add_tool_call("read_file".into(), id.clone(), HashMap::new());
        state.complete_tool_call(&id, true);
    }
    state.add_tokens(2000, 1500);
    let summary = state.completion_summary();
    assert!(summary.starts_with("Done (5 tool uses, "));
    assert!(summary.contains("3.5k tokens"));
}

#[test]
fn test_completion_summary_no_tokens() {
    let state = SubagentDisplayState::new("id-cs2".into(), "Explore".into(), "task".into());
    let summary = state.completion_summary();
    assert!(summary.starts_with("Done (0 tool uses, "));
    assert!(!summary.contains("tokens"));
}

#[test]
fn test_completion_summary_singular() {
    let mut state = SubagentDisplayState::new("id-cs3".into(), "Explore".into(), "task".into());
    state.add_tool_call("grep".into(), "tc-0".into(), HashMap::new());
    state.complete_tool_call("tc-0", true);
    let summary = state.completion_summary();
    assert!(summary.starts_with("Done (1 tool use, "));
}
