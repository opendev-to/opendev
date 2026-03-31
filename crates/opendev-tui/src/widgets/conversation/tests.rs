use super::*;
use crate::app::DisplayMessage;

#[test]
fn test_empty_conversation() {
    let msgs: Vec<DisplayMessage> = vec![];
    let widget = ConversationWidget::new(&msgs, 0);
    let lines = widget.build_lines();
    // Welcome panel is now a separate widget; empty conversation returns no lines
    assert!(lines.is_empty());
}

#[test]
fn test_user_message_rendering() {
    let msgs = vec![DisplayMessage::new(DisplayRole::User, "Hello")];
    let widget = ConversationWidget::new(&msgs, 0);
    let lines = widget.build_lines();
    assert!(lines.len() >= 2); // message + blank line
}

#[test]
fn test_tool_call_display() {
    let msgs = vec![DisplayMessage {
        role: DisplayRole::Assistant,
        content: "Running tool...".into(),
        tool_call: Some(DisplayToolCall {
            name: "bash".into(),
            arguments: std::collections::HashMap::new(),
            summary: Some("ls -la".into()),
            success: true,
            collapsed: false,
            result_lines: vec!["file1.rs".into(), "file2.rs".into()],
            nested_calls: vec![],
            error_text: None,
        }),
        collapsed: false,
        thinking_started_at: None,
        thinking_duration_secs: None,
        thinking_finalized_at: None,
    }];
    let widget = ConversationWidget::new(&msgs, 0);
    let lines = widget.build_lines();
    // message line + tool line + 2 result lines + blank
    assert!(lines.len() >= 5);
}

#[test]
fn test_system_reminder_filtered() {
    let msgs = vec![DisplayMessage::new(
        DisplayRole::Assistant,
        "Hello<system-reminder>secret</system-reminder> world",
    )];
    let widget = ConversationWidget::new(&msgs, 0);
    let lines = widget.build_lines();
    // Should not contain "secret"
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();
    assert!(!text.contains("secret"));
    assert!(text.contains("Hello"));
    assert!(text.contains("world"));
}

#[test]
fn test_collapsed_tool_result() {
    let msgs = vec![DisplayMessage {
        role: DisplayRole::Assistant,
        content: "Done".into(),
        tool_call: Some(DisplayToolCall {
            name: "read_file".into(),
            arguments: std::collections::HashMap::new(),
            summary: Some("Read 100 lines".into()),
            success: true,
            collapsed: true,
            result_lines: vec!["line1".into(), "line2".into()],
            nested_calls: vec![],
            error_text: None,
        }),
        collapsed: false,
        thinking_started_at: None,
        thinking_duration_secs: None,
        thinking_finalized_at: None,
    }];
    let widget = ConversationWidget::new(&msgs, 0);
    let lines = widget.build_lines();
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();
    // read_file shows "Read 2 lines" without parentheses or "Ctrl+O" hint
    assert!(text.contains("Read 2 lines"));
    assert!(!text.contains("Ctrl+O"));
    assert!(!text.contains("("));
}

#[test]
fn test_spinner_active_tools() {
    let msgs = vec![DisplayMessage::new(DisplayRole::User, "Do something")];
    let mut args = std::collections::HashMap::new();
    args.insert("command".into(), serde_json::Value::String("ls -la".into()));
    let tools = vec![ToolExecution {
        id: "t1".into(),
        name: "run_command".into(),
        output_lines: vec![],
        state: crate::app::ToolState::Running,
        elapsed_secs: 3,
        started_at: std::time::Instant::now(),
        tick_count: 0,
        parent_id: None,
        depth: 0,
        args,
    }];
    let widget = ConversationWidget::new(&msgs, 0).active_tools(&tools);
    let render_lines = widget.build_render_lines();
    let text: String = render_lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();
    // Should show display name like "Bash ls -la" not raw "run_command"
    assert!(text.contains("Bash"));
    assert!(text.contains("ls -la"));
    assert!(text.contains("3s"));
}

#[test]
fn test_spinner_thinking() {
    let msgs = vec![DisplayMessage::new(DisplayRole::User, "Hello")];
    let progress = TaskProgress {
        description: "Thinking".to_string(),
        elapsed_secs: 5,
        token_display: None,
        interrupted: false,
        started_at: std::time::Instant::now(),
    };
    let widget = ConversationWidget::new(&msgs, 0).task_progress(Some(&progress));
    let spinner = widget.build_spinner_lines();
    let text: String = spinner
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();
    assert!(text.contains("Thinking..."));
    assert!(text.contains("(Esc to interrupt)"));
}

#[test]
fn test_spinner_tools_take_priority_over_thinking() {
    let msgs = vec![DisplayMessage::new(DisplayRole::User, "Hello")];
    let tools = vec![ToolExecution {
        id: "t1".into(),
        name: "read_file".into(),
        output_lines: vec![],
        state: crate::app::ToolState::Running,
        elapsed_secs: 1,
        started_at: std::time::Instant::now(),
        tick_count: 2,
        parent_id: None,
        depth: 0,
        args: Default::default(),
    }];
    let progress = TaskProgress {
        description: "Thinking".to_string(),
        elapsed_secs: 5,
        token_display: None,
        interrupted: false,
        started_at: std::time::Instant::now(),
    };
    let widget = ConversationWidget::new(&msgs, 0)
        .active_tools(&tools)
        .task_progress(Some(&progress));
    let spinner = widget.build_spinner_lines();
    let text: String = spinner
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();
    // Active tools shown with display name, not thinking
    assert!(text.contains("Read"));
    assert!(!text.contains("Thinking..."));
}

#[test]
fn test_no_spinner_when_idle() {
    let msgs = vec![DisplayMessage::new(DisplayRole::User, "Hello")];
    let widget = ConversationWidget::new(&msgs, 0);
    let spinner = widget.build_spinner_lines();
    assert!(spinner.is_empty());
    // Message lines: "> Hello" + blank separator
    let lines = widget.build_lines();
    assert_eq!(lines.len(), 2);
}

#[test]
fn test_nested_tool_calls() {
    let msgs = vec![DisplayMessage {
        role: DisplayRole::Assistant,
        content: "".into(),
        tool_call: Some(DisplayToolCall {
            name: "spawn_subagent".into(),
            arguments: std::collections::HashMap::new(),
            summary: Some("Exploring codebase".into()),
            success: true,
            collapsed: false,
            result_lines: vec![],
            nested_calls: vec![DisplayToolCall {
                name: "read_file".into(),
                arguments: std::collections::HashMap::new(),
                summary: Some("src/main.rs".into()),
                success: true,
                collapsed: false,
                result_lines: vec![],
                nested_calls: vec![],
                error_text: None,
            }],
            error_text: None,
        }),
        collapsed: false,
        thinking_started_at: None,
        thinking_duration_secs: None,
        thinking_finalized_at: None,
    }];
    let widget = ConversationWidget::new(&msgs, 0);
    let lines = widget.build_lines();
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();
    // spawn_subagent renders as AgentName(task), nested calls show formatted verb
    assert!(text.contains("Agent"));
    assert!(text.contains("Read"));
}

#[test]
fn test_render_reserves_bottom_row_gap() {
    use ratatui::buffer::Buffer;

    let msgs = vec![DisplayMessage::new(DisplayRole::User, "Hello")];
    let widget = ConversationWidget::new(&msgs, 0);

    // Render into a small area
    let area = Rect::new(0, 0, 40, 10);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);

    // The last row (y=9) must be entirely blank — reserved gap
    for x in 0..40 {
        let cell = &buf[(x, 9)];
        assert_eq!(
            cell.symbol(),
            " ",
            "Bottom gap row should be blank at column {x}"
        );
    }
}

// ---------------------------------------------------------------
// TUI snapshot tests using TestBackend
// ---------------------------------------------------------------

/// Extract visible text from a ratatui Buffer, row by row.
fn buffer_text(buf: &ratatui::buffer::Buffer, area: Rect) -> Vec<String> {
    let mut rows = Vec::new();
    for y in area.y..area.bottom() {
        let mut row = String::new();
        for x in area.x..area.right() {
            row.push_str(buf[(x, y)].symbol());
        }
        rows.push(row.trim_end().to_string());
    }
    rows
}

#[test]
fn test_snapshot_empty_conversation() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let msgs: Vec<DisplayMessage> = vec![];

    terminal
        .draw(|frame| {
            let widget = ConversationWidget::new(&msgs, 0);
            frame.render_widget(widget, frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let rows = buffer_text(&buf, Rect::new(0, 0, 80, 24));
    // Empty conversation renders nothing — all rows should be blank
    for row in &rows {
        assert!(
            row.trim().is_empty(),
            "Expected blank row for empty conversation, got: {row:?}"
        );
    }
}

#[test]
fn test_snapshot_single_user_message() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let msgs = vec![DisplayMessage::new(DisplayRole::User, "What is Rust?")];

    terminal
        .draw(|frame| {
            let widget = ConversationWidget::new(&msgs, 0);
            frame.render_widget(widget, frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let rows = buffer_text(&buf, Rect::new(0, 0, 80, 24));
    // First row should contain the user prompt marker and message
    assert!(
        rows[0].contains(">") && rows[0].contains("What is Rust?"),
        "First row should show user message, got: {:?}",
        rows[0]
    );
}

#[test]
fn test_snapshot_multi_message_with_tool_call() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let msgs = vec![
        DisplayMessage::new(DisplayRole::User, "List files"),
        DisplayMessage {
            role: DisplayRole::Assistant,
            content: "I'll list the files.".into(),
            tool_call: Some(DisplayToolCall {
                name: "bash".into(),
                arguments: std::collections::HashMap::new(),
                summary: Some("ls -la".into()),
                success: true,
                collapsed: false,
                result_lines: vec!["main.rs".into(), "lib.rs".into()],
                nested_calls: vec![],
                error_text: None,
            }),
            collapsed: false,
            thinking_started_at: None,
            thinking_duration_secs: None,
        thinking_finalized_at: None,
        },
        DisplayMessage::new(DisplayRole::Assistant, "Here are the files."),
    ];

    terminal
        .draw(|frame| {
            let widget = ConversationWidget::new(&msgs, 0);
            frame.render_widget(widget, frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let all_text: String = buffer_text(&buf, Rect::new(0, 0, 80, 24)).join("\n");

    // Verify key content appears in the rendered output
    assert!(all_text.contains("List files"), "Missing user message");
    assert!(
        all_text.contains("list the files"),
        "Missing assistant content"
    );
    assert!(
        all_text.contains("main.rs"),
        "Missing tool result line 'main.rs'"
    );
    assert!(
        all_text.contains("lib.rs"),
        "Missing tool result line 'lib.rs'"
    );
    assert!(
        all_text.contains("Here are the files"),
        "Missing second assistant message"
    );
}

#[test]
fn test_snapshot_thinking_block() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let msgs = vec![
        DisplayMessage::new(DisplayRole::User, "Explain closures"),
        DisplayMessage::new(
            DisplayRole::Assistant,
            "Closures capture variables from their scope.",
        ),
    ];

    terminal
        .draw(|frame| {
            let widget = ConversationWidget::new(&msgs, 0);
            frame.render_widget(widget, frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let all_text: String = buffer_text(&buf, Rect::new(0, 0, 80, 24)).join("\n");

    assert!(
        all_text.contains("Explain closures"),
        "Missing user message"
    );
    assert!(
        all_text.contains("capture variables"),
        "Missing assistant response"
    );
}

#[test]
fn test_snapshot_scroll_indicator() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    // Create many messages to force scrolling in a small terminal
    let msgs: Vec<DisplayMessage> = (0..50)
        .map(|i| {
            DisplayMessage::new(
                if i % 2 == 0 {
                    DisplayRole::User
                } else {
                    DisplayRole::Assistant
                },
                format!("Message number {i} with enough text to occupy a line"),
            )
        })
        .collect();

    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    // Render with scroll offset > 0
    terminal
        .draw(|frame| {
            let widget = ConversationWidget::new(&msgs, 5);
            frame.render_widget(widget, frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();

    // When scrolled, the rightmost column should contain scrollbar characters
    // (▲ at top, █ for thumb, ║ for track, ▼ at bottom)
    let last_col = 79u16;
    let right_col: String = (0..10u16)
        .map(|y| buf[(last_col, y)].symbol().to_string())
        .collect();
    let scrollbar_chars = ['▲', '█', '░', '▼', '║'];
    assert!(
        right_col.chars().any(|c| scrollbar_chars.contains(&c)),
        "Expected scrollbar characters in rightmost column when scrolled, got: {right_col:?}"
    );
}

#[test]
fn test_diff_rendering_with_line_numbers() {
    let msgs = vec![DisplayMessage {
        role: DisplayRole::Assistant,
        content: "".into(),
        tool_call: Some(DisplayToolCall {
            name: "edit_file".into(),
            arguments: std::collections::HashMap::new(),
            summary: None,
            success: true,
            collapsed: false,
            result_lines: vec![
                "Edited file.rs: 1 replacement(s), 1 addition(s) and 1 removal(s)".into(),
                "--- a/file.rs".into(),
                "+++ b/file.rs".into(),
                "@@ -10,3 +10,3 @@".into(),
                " context".into(),
                "-old".into(),
                "+new".into(),
            ],
            nested_calls: vec![],
            error_text: None,
        }),
        collapsed: false,
        thinking_started_at: None,
        thinking_duration_secs: None,
        thinking_finalized_at: None,
    }];
    let widget = ConversationWidget::new(&msgs, 0);
    let lines = widget.build_lines();
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();

    // Should contain right-aligned line numbers
    assert!(text.contains("  10 "), "Should contain line number 10");
    assert!(text.contains("  11 "), "Should contain line number 11");
    // Should contain operators
    assert!(text.contains("+ new"), "Should contain '+ new'");
    assert!(text.contains("- old"), "Should contain '- old'");
    // Should contain reformatted summary
    assert!(
        text.contains("Added 1 line, removed 1 line"),
        "Should contain reformatted summary, got: {text}"
    );
    // Should NOT contain raw diff markers
    assert!(!text.contains("@@"), "Should not contain @@ hunk headers");
    assert!(!text.contains("--- a/"), "Should not contain file headers");
}

#[test]
fn test_edit_tool_never_collapsed() {
    let msgs = vec![DisplayMessage {
        role: DisplayRole::Assistant,
        content: "".into(),
        tool_call: Some(DisplayToolCall {
            name: "edit_file".into(),
            arguments: std::collections::HashMap::new(),
            summary: None,
            success: true,
            collapsed: true, // Explicitly set collapsed
            result_lines: vec![
                "Edited file.rs: 1 replacement(s), 1 addition(s) and 0 removal(s)".into(),
                "--- a/file.rs".into(),
                "+++ b/file.rs".into(),
                "@@ -1,3 +1,4 @@".into(),
                " line1".into(),
                " line2".into(),
                "+new line".into(),
                " line3".into(),
            ],
            nested_calls: vec![],
            error_text: None,
        }),
        collapsed: false,
        thinking_started_at: None,
        thinking_duration_secs: None,
        thinking_finalized_at: None,
    }];
    let widget = ConversationWidget::new(&msgs, 0);
    let lines = widget.build_lines();
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();

    // Even though collapsed=true, edit_file should always show expanded
    assert!(
        !text.contains("collapsed"),
        "edit_file should never show collapsed indicator"
    );
    assert!(
        text.contains("+ new line"),
        "edit_file should show diff content even when collapsed=true"
    );
}

/// Test that parallel subagents show "Done" when SubagentFinished arrives before ToolFinished.
/// This simulates the real event ordering: SubagentFinished#1,#2,#3 then ToolResult/ToolFinished.
#[test]
fn test_spinner_parallel_subagents_finished_before_tool() {
    use crate::widgets::nested_tool::SubagentDisplayState;

    let msgs = vec![DisplayMessage::new(
        DisplayRole::User,
        "Explore the codebase",
    )];

    // Create 3 spawn_subagent ToolExecutions (still active — not finished)
    let tasks = [
        "Search for authentication code",
        "Find database models",
        "Explore API endpoints",
    ];
    let tools: Vec<ToolExecution> = tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let mut args = std::collections::HashMap::new();
            args.insert("task".into(), serde_json::Value::String(task.to_string()));
            args.insert(
                "agent_type".into(),
                serde_json::Value::String("explore".into()),
            );
            ToolExecution {
                id: format!("t{i}"),
                name: "spawn_subagent".into(),
                output_lines: vec![],
                state: crate::app::ToolState::Running,
                elapsed_secs: 5,
                started_at: std::time::Instant::now(),
                tick_count: 0,
                parent_id: None,
                depth: 0,
                args,
            }
        })
        .collect();

    // All 3 subagents are finished (simulating SubagentFinished arriving first)
    let subagents: Vec<SubagentDisplayState> = tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let mut sa =
                SubagentDisplayState::new(format!("sa{i}"), "explore".into(), task.to_string());
            sa.parent_tool_id = Some(format!("t{i}"));
            sa.finished = true;
            sa.success = true;
            sa.tool_call_count = 3 + i;
            sa
        })
        .collect();

    let widget = ConversationWidget::new(&msgs, 0)
        .active_tools(&tools)
        .active_subagents(&subagents);
    let spinner = widget.build_spinner_lines();
    let text: String = spinner
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();

    // Each subagent should be rendered individually (no grouping)
    assert!(!text.contains("3 subagents"), "Should not group subagents");
    for task in &tasks {
        assert!(
            text.contains(task),
            "Expected task '{task}' in spinner output"
        );
    }
    // Each finished subagent should show Done individually
    assert!(text.contains("Done"));
}

/// Test that in-progress parallel subagents still show active tool calls (not "Done").
#[test]
fn test_spinner_parallel_subagents_in_progress() {
    use crate::widgets::nested_tool::{NestedToolCallState, SubagentDisplayState};

    let msgs = vec![DisplayMessage::new(DisplayRole::User, "Explore")];

    let tasks = ["Search auth code", "Find models"];
    let tools: Vec<ToolExecution> = tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let mut args = std::collections::HashMap::new();
            args.insert("task".into(), serde_json::Value::String(task.to_string()));
            args.insert(
                "agent_type".into(),
                serde_json::Value::String("explore".into()),
            );
            ToolExecution {
                id: format!("t{i}"),
                name: "spawn_subagent".into(),
                output_lines: vec![],
                state: crate::app::ToolState::Running,
                elapsed_secs: 2,
                started_at: std::time::Instant::now(),
                tick_count: 0,
                parent_id: None,
                depth: 0,
                args,
            }
        })
        .collect();

    // Subagents are NOT finished — still running with active tools
    let subagents: Vec<SubagentDisplayState> = tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let mut sa =
                SubagentDisplayState::new(format!("sa{i}"), "explore".into(), task.to_string());
            sa.parent_tool_id = Some(format!("t{i}"));
            sa.active_tools.insert(
                format!("nested_t{i}"),
                NestedToolCallState {
                    tool_name: "read_file".into(),
                    tool_id: format!("nested_t{i}"),
                    args: Default::default(),
                    started_at: std::time::Instant::now(),
                    tick: 0,
                },
            );
            sa
        })
        .collect();

    let widget = ConversationWidget::new(&msgs, 0)
        .active_tools(&tools)
        .active_subagents(&subagents);
    let spinner = widget.build_spinner_lines();
    let text: String = spinner
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();

    // Should NOT show "Done" — subagents are still running
    assert!(
        !text.contains("Done"),
        "Should not show 'Done' for in-progress subagents"
    );
    // Each subagent rendered individually with its active tool
    assert!(
        text.contains("Read"),
        "active tool 'Read' should appear in individual spinner lines"
    );
    // No grouping
    assert!(!text.contains("2 subagents"), "Should not group subagents");
}

#[test]
fn test_render_lines_include_spinner_content() {
    let msgs = vec![DisplayMessage::new(DisplayRole::User, "Do something")];
    let mut args = std::collections::HashMap::new();
    args.insert("command".into(), serde_json::Value::String("ls -la".into()));
    let tools = vec![ToolExecution {
        id: "t1".into(),
        name: "run_command".into(),
        output_lines: vec![],
        state: crate::app::ToolState::Running,
        elapsed_secs: 3,
        started_at: std::time::Instant::now(),
        tick_count: 0,
        parent_id: None,
        depth: 0,
        args,
    }];
    let widget = ConversationWidget::new(&msgs, 0).active_tools(&tools);
    let text: String = widget
        .build_render_lines()
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();
    assert!(text.contains("> Do something") || text.contains("Do something"));
    assert!(text.contains("Bash"));
    assert!(text.contains("3s"));
}

#[test]
fn test_snapshot_parallel_subagents_group_visible_in_tui() {
    use crate::widgets::nested_tool::SubagentDisplayState;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend).unwrap();

    let msgs = vec![DisplayMessage::new(DisplayRole::User, "Explore the repo")];

    let tasks = [
        "Search auth code",
        "Find database models",
        "Explore API routes",
        "Trace background jobs",
    ];

    let tools: Vec<ToolExecution> = tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let mut args = std::collections::HashMap::new();
            args.insert("task".into(), serde_json::Value::String(task.to_string()));
            args.insert(
                "description".into(),
                serde_json::Value::String(task.to_string()),
            );
            ToolExecution {
                id: format!("t{i}"),
                name: "spawn_subagent".into(),
                output_lines: vec![],
                state: crate::app::ToolState::Running,
                elapsed_secs: 2,
                started_at: std::time::Instant::now(),
                tick_count: 0,
                parent_id: None,
                depth: 0,
                args,
            }
        })
        .collect();

    let subagents: Vec<SubagentDisplayState> = tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let mut sa =
                SubagentDisplayState::new(format!("sa{i}"), "explore".into(), task.to_string());
            sa.parent_tool_id = Some(format!("t{i}"));
            sa
        })
        .collect();

    terminal
        .draw(|frame| {
            let widget = ConversationWidget::new(&msgs, 0)
                .active_tools(&tools)
                .active_subagents(&subagents);
            frame.render_widget(widget, frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let all_text = buffer_text(&buf, Rect::new(0, 0, 60, 8)).join("\n");

    // Each subagent rendered individually (no grouping)
    assert!(
        !all_text.contains("4 subagents"),
        "Should not group subagents: {all_text}"
    );
    // At least some individual agents visible in the 8-row viewport
    let visible = tasks.iter().filter(|t| all_text.contains(*t)).count();
    assert!(
        visible >= 1,
        "At least one subagent should be visible: {all_text}"
    );
}

#[test]
fn test_reasoning_message_visible() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let msgs = vec![
        DisplayMessage {
            role: DisplayRole::Reasoning,
            content:
                "Let me think step by step.\nFirst analyze the problem.\nThen find a solution."
                    .into(),
            tool_call: None,
            collapsed: false,
            thinking_started_at: None,
            thinking_duration_secs: Some(5),
        thinking_finalized_at: None,
        },
        DisplayMessage::new(DisplayRole::Assistant, "The answer is 42."),
    ];

    terminal
        .draw(|frame| {
            let widget = ConversationWidget::new(&msgs, 0);
            frame.render_widget(widget, frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let rows = buffer_text(&buf, Rect::new(0, 0, 80, 24));
    let all_text = rows.join("\n");

    // Thinking content should be visible
    assert!(
        all_text.contains("think step by step"),
        "Reasoning content missing from render. Rows:\n{}",
        rows.iter()
            .enumerate()
            .map(|(i, r)| format!("  [{i:2}] {r:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Assistant content should also be visible
    assert!(
        all_text.contains("answer is 42"),
        "Assistant content missing from render"
    );
    // Check for thinking icon
    assert!(
        all_text.contains('⟡'),
        "Missing ⟡ thinking icon. Rows:\n{}",
        rows.iter()
            .enumerate()
            .map(|(i, r)| format!("  [{i:2}] {r:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
