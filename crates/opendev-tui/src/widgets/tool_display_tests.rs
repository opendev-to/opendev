use super::*;
use crate::app::ToolState;
use std::time::Instant;

#[test]
fn test_empty_tool_display() {
    let tools: Vec<ToolExecution> = vec![];
    let _widget = ToolDisplayWidget::new(&tools);
}

#[test]
fn test_tool_display_with_output() {
    let tools = vec![ToolExecution {
        id: "t1".into(),
        name: "bash".into(),
        output_lines: vec!["file1.rs".into(), "file2.rs".into()],
        state: ToolState::Running,
        elapsed_secs: 3,
        started_at: Instant::now(),
        tick_count: 0,
        parent_id: None,
        depth: 0,
        args: Default::default(),
    }];
    let _widget = ToolDisplayWidget::new(&tools);
}

#[test]
fn test_tool_display_nested() {
    let tools = vec![
        ToolExecution {
            id: "t1".into(),
            name: "spawn_subagent".into(),
            output_lines: vec![],
            state: ToolState::Running,
            elapsed_secs: 5,
            started_at: Instant::now(),
            tick_count: 0,
            parent_id: None,
            depth: 0,
            args: Default::default(),
        },
        ToolExecution {
            id: "t2".into(),
            name: "read_file".into(),
            output_lines: vec!["reading...".into()],
            state: ToolState::Running,
            elapsed_secs: 2,
            started_at: Instant::now(),
            tick_count: 0,
            parent_id: Some("t1".into()),
            depth: 1,
            args: Default::default(),
        },
    ];
    let _widget = ToolDisplayWidget::new(&tools);
}
