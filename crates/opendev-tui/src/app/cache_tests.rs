use super::super::*;
use super::*;

#[test]
fn test_viewport_culling_cached_lines() {
    let mut app = App::new();
    // Add many messages
    for i in 0..100 {
        app.state.messages.push(DisplayMessage::new(
            DisplayRole::User,
            format!("Message {i}"),
        ));
    }
    app.state.message_generation = 1;
    app.state.terminal_height = 24;
    app.state.scroll_offset = 0;

    // Build cached lines
    app.rebuild_cached_lines();

    // Should have lines for all messages (some may be placeholders)
    assert!(
        !app.state.cached_lines.is_empty(),
        "cached_lines should not be empty"
    );
}

// ---------------------------------------------------------------
// Per-message dirty tracking tests
// ---------------------------------------------------------------

#[test]

fn test_markdown_cache_hit() {
    let mut app = App::new();
    app.state.messages.push(DisplayMessage::new(
        DisplayRole::Assistant,
        "Hello **world**",
    ));
    app.state.terminal_height = 24;
    app.rebuild_cached_lines();
    assert_eq!(app.state.markdown_cache.len(), 1);
    let first_lines = app.state.cached_lines.clone();
    app.state.per_message_hashes.clear();
    app.state.per_message_line_counts.clear();
    app.state.cached_lines.clear();
    app.rebuild_cached_lines();
    assert_eq!(app.state.markdown_cache.len(), 1);
    assert_eq!(app.state.cached_lines.len(), first_lines.len());
}

#[test]
fn test_markdown_cache_miss_different_content() {
    let mut app = App::new();
    app.state.messages.push(DisplayMessage::new(
        DisplayRole::Assistant,
        "Hello **world**",
    ));
    app.state.terminal_height = 24;
    app.rebuild_cached_lines();
    assert_eq!(app.state.markdown_cache.len(), 1);
    app.state.messages[0].content = "Goodbye **world**".into();
    app.rebuild_cached_lines();
    assert_eq!(app.state.markdown_cache.len(), 2);
}

#[test]
fn test_markdown_cache_clear() {
    let mut app = App::new();
    app.state.messages.push(DisplayMessage::new(
        DisplayRole::Assistant,
        "# Title\nSome text",
    ));
    app.state.terminal_height = 24;
    app.rebuild_cached_lines();
    assert!(!app.state.markdown_cache.is_empty());
    app.clear_markdown_cache();
    assert!(app.state.markdown_cache.is_empty());
}

#[test]

fn test_incremental_append_only_renders_new_message() {
    let mut app = App::new();
    app.state.terminal_height = 24;
    app.state
        .messages
        .push(DisplayMessage::new(DisplayRole::User, "First message"));
    app.rebuild_cached_lines();
    let lines_after_first = app.state.cached_lines.len();
    assert!(lines_after_first > 0);
    assert_eq!(app.state.per_message_hashes.len(), 1);
    assert_eq!(app.state.per_message_line_counts.len(), 1);
    let first_hash = app.state.per_message_hashes[0];
    let first_lines_snapshot = app.state.cached_lines.clone();

    // Append a second message
    app.state
        .messages
        .push(DisplayMessage::new(DisplayRole::User, "Second message"));
    app.rebuild_cached_lines();
    assert_eq!(
        app.state.per_message_hashes[0], first_hash,
        "first message hash should be unchanged after append"
    );
    assert_eq!(app.state.per_message_hashes.len(), 2);
    for i in 0..first_lines_snapshot.len() {
        assert_eq!(
            format!("{:?}", app.state.cached_lines[i]),
            format!("{:?}", first_lines_snapshot[i]),
            "first message lines should be preserved at index {i}"
        );
    }
    assert!(app.state.cached_lines.len() > lines_after_first);
}

#[test]
fn test_incremental_modify_middle_rebuilds_from_change() {
    let mut app = App::new();
    app.state.terminal_height = 24;
    for content in &["First", "Second", "Third"] {
        app.state
            .messages
            .push(DisplayMessage::new(DisplayRole::User, content.to_string()));
    }
    app.rebuild_cached_lines();
    let original_lines = app.state.cached_lines.len();
    assert_eq!(app.state.per_message_hashes.len(), 3);
    let first_hash = app.state.per_message_hashes[0];
    let first_line_count = app.state.per_message_line_counts[0];

    // Modify the second message
    app.state.messages[1].content = "Modified Second".into();
    app.rebuild_cached_lines();

    // First message preserved
    assert_eq!(app.state.per_message_hashes[0], first_hash);
    assert_eq!(app.state.per_message_line_counts[0], first_line_count);
    assert_eq!(app.state.per_message_hashes.len(), 3);
    // Second hash changed
    assert_ne!(
        app.state.per_message_hashes[1],
        display_message_hash(&DisplayMessage::new(DisplayRole::User, "Second")),
    );
    assert_eq!(app.state.cached_lines.len(), original_lines);
}

#[test]
fn test_incremental_empty_conversation() {
    let mut app = App::new();
    app.state.terminal_height = 24;
    app.rebuild_cached_lines();
    assert!(app.state.cached_lines.is_empty());
    assert!(app.state.per_message_hashes.is_empty());
    assert!(app.state.per_message_line_counts.is_empty());
}

#[test]
fn test_incremental_multiple_appends_correct_cache() {
    let mut app = App::new();
    app.state.terminal_height = 24;
    for i in 0..5u32 {
        app.state.messages.push(DisplayMessage::new(
            if i % 2 == 0 {
                DisplayRole::User
            } else {
                DisplayRole::Assistant
            },
            format!("Message {i}"),
        ));
        app.rebuild_cached_lines();
        assert_eq!(app.state.per_message_hashes.len(), (i + 1) as usize);
        assert_eq!(app.state.per_message_line_counts.len(), (i + 1) as usize);
    }
    // Compare with full rebuild
    let incremental_lines = app.state.cached_lines.clone();
    app.state.per_message_hashes.clear();
    app.state.per_message_line_counts.clear();
    app.state.cached_lines.clear();
    app.rebuild_cached_lines();
    assert_eq!(app.state.cached_lines.len(), incremental_lines.len());
    for (i, (inc, full)) in incremental_lines
        .iter()
        .zip(app.state.cached_lines.iter())
        .enumerate()
    {
        assert_eq!(
            format!("{:?}", inc),
            format!("{:?}", full),
            "line {i} differs between incremental and full rebuild"
        );
    }
}

#[test]
fn test_incremental_no_change_is_noop() {
    let mut app = App::new();
    app.state.terminal_height = 24;
    app.state
        .messages
        .push(DisplayMessage::new(DisplayRole::User, "Hello"));
    app.rebuild_cached_lines();
    let lines_after = app.state.cached_lines.clone();
    // Second rebuild with no changes
    app.rebuild_cached_lines();
    assert_eq!(app.state.cached_lines.len(), lines_after.len());
}

#[test]
fn test_incremental_message_removal() {
    let mut app = App::new();
    app.state.terminal_height = 24;
    app.state
        .messages
        .push(DisplayMessage::new(DisplayRole::User, "First"));
    app.state
        .messages
        .push(DisplayMessage::new(DisplayRole::User, "Second"));
    app.rebuild_cached_lines();
    assert_eq!(app.state.per_message_hashes.len(), 2);
    app.state.messages.pop();
    app.rebuild_cached_lines();
    assert_eq!(app.state.per_message_hashes.len(), 1);
    assert_eq!(app.state.per_message_line_counts.len(), 1);
}

#[test]
fn test_reasoning_message_produces_lines() {
    let mut app = App::new();
    app.state.terminal_height = 24;
    app.state.terminal_width = 120;
    app.state.messages.push(DisplayMessage {
        role: DisplayRole::Reasoning,
        content: "Let me think about this.\nFirst, I need to understand.\nThen solve.".into(),
        tool_call: None,
        collapsed: false,
        thinking_started_at: None,
        thinking_duration_secs: Some(5),
        thinking_finalized_at: None,
    });
    app.rebuild_cached_lines();

    // Should have produced lines
    assert!(
        !app.state.cached_lines.is_empty(),
        "reasoning message should produce cached lines"
    );
    assert!(
        app.state.cached_lines.len() >= 3,
        "expected at least 3 lines (3 content + blank), got {}",
        app.state.cached_lines.len()
    );

    // Check that first line has ⟡ prefix
    let first_line = &app.state.cached_lines[0];
    let first_text: String = first_line
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(
        first_text.contains('⟡'),
        "first line should have ⟡ icon, got: {first_text}"
    );

    // Check that continuation lines have │ prefix
    for (i, line) in app.state.cached_lines.iter().enumerate().skip(1) {
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        if text.trim().is_empty() {
            continue; // blank separator line
        }
        assert!(
            text.starts_with('│'),
            "line {i} should start with │, got: {text:?}"
        );
    }

    // Content should be preserved
    let all_text: String = app
        .state
        .cached_lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();
    assert!(
        all_text.contains("think about this"),
        "content lost: {all_text}"
    );
}

#[test]
fn test_reasoning_via_cached_lines_widget() {
    use crate::widgets::conversation::ConversationWidget;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut app = App::new();
    app.state.terminal_width = 80;
    app.state.terminal_height = 24;
    app.state.messages.push(DisplayMessage {
        role: DisplayRole::Reasoning,
        content: "Thinking carefully about the problem at hand.".into(),
        tool_call: None,
        collapsed: false,
        thinking_started_at: None,
        thinking_duration_secs: Some(5),
        thinking_finalized_at: None,
    });
    app.state.messages.push(DisplayMessage::new(
        DisplayRole::Assistant,
        "The result is clear.",
    ));
    app.state.message_generation = 1;
    app.rebuild_cached_lines();

    assert!(
        !app.state.cached_lines.is_empty(),
        "cached lines should not be empty after rebuild"
    );

    // Render with cached lines through the widget
    let cached = app.state.cached_lines.clone();
    let msgs = app.state.messages.clone();
    terminal
        .draw(|frame| {
            let widget = ConversationWidget::new(&msgs, 0).cached_lines(&cached);
            frame.render_widget(widget, frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let mut all_text = String::new();
    for y in 0..24u16 {
        for x in 0..80u16 {
            if let Some(cell) = buf.cell(ratatui::layout::Position::new(x, y)) {
                all_text.push_str(cell.symbol());
            }
        }
        all_text.push('\n');
    }

    assert!(
        all_text.contains("Thinking carefully"),
        "Cached reasoning content missing from rendered buffer.\nBuffer:\n{all_text}"
    );
    assert!(
        all_text.contains("result is clear"),
        "Cached assistant content missing from rendered buffer.\nBuffer:\n{all_text}"
    );
}

#[test]
fn test_active_subagents_keep_recent_reasoning_lines_cached() {
    let mut app = App::new();
    app.state.terminal_width = 60;
    app.state.terminal_height = 12;
    for i in 0..40 {
        app.state.messages.push(DisplayMessage::new(
            DisplayRole::User,
            format!("Older message {i}"),
        ));
    }
    app.state.messages.push(DisplayMessage {
        role: DisplayRole::Reasoning,
        content: "Thinking through how to split this work safely.".into(),
        tool_call: None,
        collapsed: false,
        thinking_started_at: None,
        thinking_duration_secs: Some(5),
        thinking_finalized_at: None,
    });
    app.state.messages.push(DisplayMessage::new(
        DisplayRole::Assistant,
        "I will spawn 2 agents to explore the codebase.",
    ));
    app.state.agent_active = true;
    app.state.active_tools.push(ToolExecution {
        id: "t0".into(),
        name: "spawn_subagent".into(),
        output_lines: vec![],
        state: ToolState::Running,
        elapsed_secs: 1,
        started_at: std::time::Instant::now(),
        tick_count: 0,
        parent_id: None,
        depth: 0,
        args: std::collections::HashMap::new(),
    });
    app.state.message_generation = 1;
    app.rebuild_cached_lines();

    let all_text: String = app
        .state
        .cached_lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();
    assert!(
        all_text.contains("Thinking through"),
        "recent reasoning line was culled while tools were active: {all_text}"
    );
    assert!(
        all_text.contains("spawn 2 agents"),
        "recent assistant line was culled while tools were active: {all_text}"
    );
}

#[test]
fn test_reasoning_to_subagent_transition_remains_visible_in_short_tui() {
    use crate::widgets::conversation::ConversationWidget;
    use crate::widgets::nested_tool::SubagentDisplayState;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(60, 16);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut app = App::new();
    app.state.terminal_width = 60;
    app.state.terminal_height = 16;
    app.state.messages.push(DisplayMessage {
        role: DisplayRole::Reasoning,
        content: "Thinking through the codebase structure.".into(),
        tool_call: None,
        collapsed: false,
        thinking_started_at: None,
        thinking_duration_secs: Some(5),
        thinking_finalized_at: None,
    });
    app.state.messages.push(DisplayMessage::new(
        DisplayRole::Assistant,
        "I will spawn 2 agents to explore the codebase.",
    ));

    let tasks = ["Inspect auth flow", "Trace API routes"];
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
                state: ToolState::Running,
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

    app.state.active_tools = tools.clone();
    app.state.active_subagents = subagents.clone();
    app.state.message_generation = 1;
    app.rebuild_cached_lines();

    let cached = app.state.cached_lines.clone();
    let msgs = app.state.messages.clone();
    terminal
        .draw(|frame| {
            let widget = ConversationWidget::new(&msgs, 0)
                .cached_lines(&cached)
                .active_tools(&tools)
                .active_subagents(&subagents);
            frame.render_widget(widget, frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let mut all_text = String::new();
    for y in 0..16u16 {
        for x in 0..60u16 {
            if let Some(cell) = buf.cell(ratatui::layout::Position::new(x, y)) {
                all_text.push_str(cell.symbol());
            }
        }
        all_text.push('\n');
    }

    assert!(
        all_text.contains("spawn 2 agents"),
        "assistant handoff text disappeared from TUI.\nBuffer:\n{all_text}"
    );
    // Each subagent should be rendered individually (no grouping)
    assert!(
        all_text.contains("Inspect auth flow") || all_text.contains("Trace API routes"),
        "individual subagent spinners missing from TUI.\nBuffer:\n{all_text}"
    );
    assert!(
        !all_text.contains("2 subagents"),
        "subagents should not be grouped.\nBuffer:\n{all_text}"
    );
}

#[test]
fn test_event_sequence_reasoning_then_spawn_subagent_keeps_context() {
    use crate::event::AppEvent;

    let mut app = App::new();
    app.state.terminal_width = 60;
    app.state.terminal_height = 8;

    app.handle_event(AppEvent::ReasoningContent(
        "Thinking through the codebase structure.".into(),
    ));
    app.handle_event(AppEvent::AgentChunk(
        "I will spawn 2 agents to explore the codebase.".into(),
    ));
    app.handle_event(AppEvent::ToolStarted {
        tool_id: "t1".into(),
        tool_name: "spawn_subagent".into(),
        args: {
            let mut args = std::collections::HashMap::new();
            args.insert(
                "task".into(),
                serde_json::Value::String("Inspect auth flow".into()),
            );
            args.insert(
                "description".into(),
                serde_json::Value::String("Inspect auth flow".into()),
            );
            args
        },
    });
    app.handle_event(AppEvent::ToolStarted {
        tool_id: "t2".into(),
        tool_name: "spawn_subagent".into(),
        args: {
            let mut args = std::collections::HashMap::new();
            args.insert(
                "task".into(),
                serde_json::Value::String("Trace API routes".into()),
            );
            args.insert(
                "description".into(),
                serde_json::Value::String("Trace API routes".into()),
            );
            args
        },
    });

    app.rebuild_cached_lines();
    let all_text: String = app
        .state
        .cached_lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.to_string())
        .collect();
    assert!(
        all_text.contains("Thought for"),
        "collapsed reasoning context disappeared after ToolStarted events: {all_text}"
    );
    assert!(
        all_text.contains("spawn 2 agents"),
        "assistant handoff disappeared after ToolStarted events: {all_text}"
    );
    assert_eq!(app.state.active_tools.len(), 2);
    assert_eq!(app.state.active_subagents.len(), 2);
}

#[test]
fn test_25_subagents_render_to_terminal() {
    use crate::widgets::conversation::ConversationWidget;
    use crate::widgets::nested_tool::SubagentDisplayState;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(120, 80);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut app = App::new();
    app.state.terminal_width = 120;
    app.state.terminal_height = 80;
    app.state.messages.push(DisplayMessage {
        role: DisplayRole::Reasoning,
        content: "Planning 25 parallel explorations.".into(),
        tool_call: None,
        collapsed: false,
        thinking_started_at: None,
        thinking_duration_secs: Some(5),
        thinking_finalized_at: None,
    });
    app.state.messages.push(DisplayMessage::new(
        DisplayRole::Assistant,
        "I will spawn 25 agents to explore the codebase.",
    ));

    let tools: Vec<ToolExecution> = (0..25)
        .map(|i| {
            let mut args = std::collections::HashMap::new();
            args.insert(
                "task".into(),
                serde_json::Value::String(format!("Explore area {i}")),
            );
            args.insert(
                "description".into(),
                serde_json::Value::String(format!("Explore area {i}")),
            );
            ToolExecution {
                id: format!("t{i}"),
                name: "spawn_subagent".into(),
                output_lines: vec![],
                state: ToolState::Running,
                elapsed_secs: 2,
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
                "explore".into(),
                format!("Explore area {i}"),
            );
            sa.parent_tool_id = Some(format!("t{i}"));
            sa
        })
        .collect();

    app.state.active_tools = tools.clone();
    app.state.active_subagents = subagents.clone();
    app.state.agent_active = true;
    app.state.message_generation = 1;
    app.rebuild_cached_lines();

    let cached = app.state.cached_lines.clone();
    let msgs = app.state.messages.clone();
    terminal
        .draw(|frame| {
            let widget = ConversationWidget::new(&msgs, 0)
                .cached_lines(&cached)
                .active_tools(&tools)
                .active_subagents(&subagents);
            frame.render_widget(widget, frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let mut all_text = String::new();
    for y in 0..80u16 {
        for x in 0..120u16 {
            if let Some(cell) = buf.cell(ratatui::layout::Position::new(x, y)) {
                all_text.push_str(cell.symbol());
            }
        }
        all_text.push('\n');
    }

    // At least 20 of the 25 agent descriptions should appear (accounting for scroll)
    let mut found = 0;
    for i in 0..25 {
        if all_text.contains(&format!("Explore area {i}")) {
            found += 1;
        }
    }
    assert!(
        found >= 20,
        "expected at least 20 of 25 agent descriptions visible, found {found}.\nBuffer:\n{all_text}"
    );

    // No grouping text
    assert!(
        !all_text.contains("25 subagents"),
        "should not contain grouped subagent text.\nBuffer:\n{all_text}"
    );

    // Assistant handoff text should still be visible
    assert!(
        all_text.contains("spawn 25 agents"),
        "assistant handoff text should be visible.\nBuffer:\n{all_text}"
    );
}

// -- Slash command argument parsing tests --
