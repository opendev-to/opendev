use super::super::ConversationWidget;
use crate::app::{DisplayMessage, DisplayRole, ToolExecution, ToolState};
use crate::widgets::nested_tool::SubagentDisplayState;

#[test]
fn test_25_subagents_all_rendered_individually() {
    let msgs: Vec<DisplayMessage> = vec![DisplayMessage::new(
        DisplayRole::Assistant,
        "Spawning 25 agents.",
    )];

    let tools: Vec<ToolExecution> = (0..25)
        .map(|i| {
            let mut args = std::collections::HashMap::new();
            args.insert(
                "task".into(),
                serde_json::Value::String(format!("Task_{i}")),
            );
            args.insert(
                "description".into(),
                serde_json::Value::String(format!("Task_{i}")),
            );
            args.insert(
                "agent_type".into(),
                serde_json::Value::String(format!("agent_{i}")),
            );
            ToolExecution {
                id: format!("t{i}"),
                name: "spawn_subagent".into(),
                output_lines: vec![],
                state: ToolState::Running,
                elapsed_secs: 1,
                started_at: std::time::Instant::now(),
                tick_count: 0,
                parent_id: None,
                depth: 0,
                args,
            }
        })
        .collect();

    let subagents: Vec<SubagentDisplayState> = (0..25)
        .map(|i| {
            let mut sa = SubagentDisplayState::new(
                format!("sa{i}"),
                format!("agent_{i}"),
                format!("Task_{i}"),
            );
            sa.parent_tool_id = Some(format!("t{i}"));
            sa
        })
        .collect();

    let widget = ConversationWidget::new(&msgs, 0)
        .active_tools(&tools)
        .active_subagents(&subagents);

    let lines = widget.build_spinner_lines();
    let all_text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();

    // No grouping header
    assert!(
        !all_text.contains("subagents"),
        "should not contain grouped 'subagents' text, got: {all_text}"
    );

    // All 25 agents rendered individually
    for i in 0..25 {
        assert!(
            all_text.contains(&format!("Task_{i}")),
            "agent Task_{i} missing from spinner output"
        );
    }
}
