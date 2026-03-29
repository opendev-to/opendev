use super::*;

fn make_items() -> Vec<TodoDisplayItem> {
    vec![
        TodoDisplayItem {
            id: 1,
            title: "Set up project".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
        TodoDisplayItem {
            id: 2,
            title: "Write code".into(),
            status: TodoDisplayStatus::InProgress,
            active_form: Some("Writing code".into()),
        },
        TodoDisplayItem {
            id: 3,
            title: "Write tests".into(),
            status: TodoDisplayStatus::Pending,
            active_form: None,
        },
    ]
}

#[test]
fn test_build_lines_count() {
    let items = make_items();
    let widget = TodoPanelWidget::new(&items);
    let (done, in_progress, total) = widget.counts();
    let lines = widget.build_lines(done, in_progress, total);
    // 3 item lines (no progress bar)
    assert_eq!(lines.len(), 3);
}

#[test]
fn test_render_does_not_panic() {
    let items = make_items();
    let widget = TodoPanelWidget::new(&items).with_plan_name("bold-blazing-badger");
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 10));
    widget.render(Rect::new(0, 0, 80, 10), &mut buf);
}

#[test]
fn test_empty_items() {
    let items: Vec<TodoDisplayItem> = vec![];
    let widget = TodoPanelWidget::new(&items);
    let (done, in_progress, total) = widget.counts();
    let lines = widget.build_lines(done, in_progress, total);
    assert!(lines.is_empty());
}

#[test]
fn test_all_completed_green_border() {
    let items = vec![
        TodoDisplayItem {
            id: 1,
            title: "Done".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
        TodoDisplayItem {
            id: 2,
            title: "Also done".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
    ];
    // Just verify no panic with all completed
    let widget = TodoPanelWidget::new(&items);
    let mut buf = Buffer::empty(Rect::new(0, 0, 60, 6));
    widget.render(Rect::new(0, 0, 60, 6), &mut buf);
}

#[test]
fn test_long_title_not_truncated() {
    let items = vec![TodoDisplayItem {
        id: 1,
        title: "A".repeat(100),
        status: TodoDisplayStatus::Pending,
        active_form: None,
    }];
    let widget = TodoPanelWidget::new(&items);
    let (done, in_progress, total) = widget.counts();
    let lines = widget.build_lines(done, in_progress, total);
    assert_eq!(lines.len(), 1);
    // Full title should be present (no truncation)
    let text: String = lines[0]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(text.contains(&"A".repeat(100)));
}

#[test]
fn test_collapsed_render() {
    let items = make_items();
    let widget = TodoPanelWidget::new(&items)
        .with_expanded(false)
        .with_spinner_tick(3);
    let mut buf = Buffer::empty(Rect::new(0, 0, 60, 3));
    widget.render(Rect::new(0, 0, 60, 3), &mut buf);
}

#[test]
fn test_collapsed_uses_active_form() {
    let items = make_items();
    let widget = TodoPanelWidget::new(&items).with_expanded(false);
    let (done, _, total) = widget.counts();
    let line = widget.build_collapsed_line(done, total);
    // Should contain the active_form text "Writing code"
    let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
    assert!(text.contains("Writing code"));
}

#[test]
fn test_required_height_expanded() {
    let items = make_items();
    let widget = TodoPanelWidget::new(&items);
    // 3 items + 2 borders = 5
    assert_eq!(widget.required_height(), 5);
}

#[test]
fn test_collapsed_all_done_shows_checkmark() {
    let items = vec![
        TodoDisplayItem {
            id: 1,
            title: "Task A".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
        TodoDisplayItem {
            id: 2,
            title: "Task B".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
    ];
    let widget = TodoPanelWidget::new(&items).with_expanded(false);
    let (done, _, total) = widget.counts();
    let line = widget.build_collapsed_line(done, total);
    let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
    assert!(
        text.contains("All tasks complete"),
        "Expected 'All tasks complete', got: {text}"
    );
    assert!(text.contains('\u{2714}'), "Expected checkmark in: {text}");
    assert!(
        !text.contains("Working"),
        "Should not show 'Working' when all done"
    );
}

#[test]
fn test_required_height_zero_when_all_done() {
    let items = vec![
        TodoDisplayItem {
            id: 1,
            title: "Done".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
        TodoDisplayItem {
            id: 2,
            title: "Also done".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
    ];
    let widget = TodoPanelWidget::new(&items);
    assert_eq!(
        widget.required_height(),
        0,
        "Panel should hide (height 0) when all items are completed"
    );
}

#[test]
fn test_required_height_collapsed() {
    let items = make_items();
    let widget = TodoPanelWidget::new(&items).with_expanded(false);
    assert_eq!(widget.required_height(), 3);
}

#[test]
fn test_expanded_title_no_spinner_in_header() {
    let items = make_items(); // has 1 in-progress item
    let widget = TodoPanelWidget::new(&items).with_spinner_tick(2);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 10));
    widget.render(Rect::new(0, 0, 80, 10), &mut buf);
    // Extract top row text from buffer
    let top_row: String = (0..80)
        .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
        .collect::<String>();
    // No spinner in the expanded header (spinners are per-item only)
    for frame in SPINNER_FRAMES {
        assert!(
            !top_row.contains(*frame),
            "Should not have spinner in expanded header, got: {top_row}"
        );
    }
    assert!(
        top_row.contains("Ctrl+T to toggle"),
        "Expected hint in title, got: {top_row}"
    );
}

#[test]
fn test_expanded_title_no_spinner_when_all_done() {
    let items = vec![TodoDisplayItem {
        id: 1,
        title: "Done".into(),
        status: TodoDisplayStatus::Completed,
        active_form: None,
    }];
    let widget = TodoPanelWidget::new(&items).with_spinner_tick(2);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 6));
    widget.render(Rect::new(0, 0, 80, 6), &mut buf);
    let top_row: String = (0..80)
        .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
        .collect::<String>();
    for frame in SPINNER_FRAMES {
        assert!(
            !top_row.contains(*frame),
            "Should not have spinner when all done, got: {top_row}"
        );
    }
    assert!(
        top_row.contains("Ctrl+T to toggle"),
        "Expected hint in title, got: {top_row}"
    );
}

#[test]
fn test_todo_panel_height_helper() {
    assert_eq!(todo_panel_height(0, true), 0);
    assert_eq!(todo_panel_height(0, false), 0);
    assert_eq!(todo_panel_height(3, true), 5); // 3 + 2 borders
    assert_eq!(todo_panel_height(3, false), 3); // collapsed
    assert_eq!(todo_panel_height(10, true), 12); // 10 + 2 = 12 (at cap)
    assert_eq!(todo_panel_height(15, true), 12); // capped at 12
}

// --- Widget rendering tests ---

#[test]
fn test_spinner_frame_changes_with_tick() {
    let items = vec![TodoDisplayItem {
        id: 1,
        title: "Active".into(),
        status: TodoDisplayStatus::InProgress,
        active_form: None,
    }];
    // Different ticks should produce different spinner characters
    let widget0 = TodoPanelWidget::new(&items).with_spinner_tick(0);
    let widget1 = TodoPanelWidget::new(&items).with_spinner_tick(1);
    let (d, ip, t) = widget0.counts();
    let line0 = widget0.build_lines(d, ip, t);
    let (d, ip, t) = widget1.counts();
    let line1 = widget1.build_lines(d, ip, t);

    let text0: String = line0[0]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    let text1: String = line1[0]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert_ne!(
        text0, text1,
        "Different ticks should produce different spinner chars"
    );
}

#[test]
fn test_pending_shows_circle_not_spinner() {
    let items = vec![TodoDisplayItem {
        id: 1,
        title: "Waiting".into(),
        status: TodoDisplayStatus::Pending,
        active_form: None,
    }];
    let widget = TodoPanelWidget::new(&items).with_spinner_tick(0);
    let (d, ip, t) = widget.counts();
    let lines = widget.build_lines(d, ip, t);
    let text: String = lines[0]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(
        text.contains('\u{25CB}'),
        "Pending should show ○, got: {text}"
    );
    for frame in SPINNER_FRAMES {
        assert!(
            !text.contains(*frame),
            "Pending should not show spinner {frame}, got: {text}"
        );
    }
}

#[test]
fn test_resume_shows_spinner() {
    // Simulate: item was Pending (after interrupt), then set back to InProgress
    let items = vec![
        TodoDisplayItem {
            id: 1,
            title: "Done".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
        TodoDisplayItem {
            id: 2,
            title: "Resumed task".into(),
            status: TodoDisplayStatus::InProgress,
            active_form: Some("Resuming task".into()),
        },
        TodoDisplayItem {
            id: 3,
            title: "Later".into(),
            status: TodoDisplayStatus::Pending,
            active_form: None,
        },
    ];
    let widget = TodoPanelWidget::new(&items).with_spinner_tick(3);
    let (d, ip, t) = widget.counts();
    let lines = widget.build_lines(d, ip, t);

    // Item 2 (index 1) should show spinner
    let text1: String = lines[1]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    let expected_spinner = SPINNER_FRAMES[3 % SPINNER_FRAMES.len()];
    assert!(
        text1.contains(expected_spinner),
        "Resumed InProgress item should show spinner '{expected_spinner}', got: {text1}"
    );
}

#[test]
fn test_mixed_states_render() {
    let items = vec![
        TodoDisplayItem {
            id: 1,
            title: "Completed".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
        TodoDisplayItem {
            id: 2,
            title: "Active".into(),
            status: TodoDisplayStatus::InProgress,
            active_form: Some("Working".into()),
        },
        TodoDisplayItem {
            id: 3,
            title: "Waiting".into(),
            status: TodoDisplayStatus::Pending,
            active_form: None,
        },
    ];
    let widget = TodoPanelWidget::new(&items).with_spinner_tick(0);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 7));
    widget.render(Rect::new(0, 0, 80, 7), &mut buf);

    // Extract item rows (row 0 is title border, items start at row 1)
    let row1: String = (0..80)
        .map(|x| buf.cell((x, 1)).unwrap().symbol().to_string())
        .collect();
    let row2: String = (0..80)
        .map(|x| buf.cell((x, 2)).unwrap().symbol().to_string())
        .collect();
    let row3: String = (0..80)
        .map(|x| buf.cell((x, 3)).unwrap().symbol().to_string())
        .collect();

    assert!(
        row1.contains('\u{2714}'),
        "Completed should show ✔, got: {row1}"
    );
    assert!(
        row2.contains(SPINNER_FRAMES[0]),
        "InProgress should show spinner, got: {row2}"
    );
    assert!(
        row3.contains('\u{25CB}'),
        "Pending should show ○, got: {row3}"
    );
}

#[test]
fn test_collapsed_no_in_progress_shows_working() {
    let items = vec![TodoDisplayItem {
        id: 1,
        title: "Task A".into(),
        status: TodoDisplayStatus::Pending,
        active_form: None,
    }];
    let widget = TodoPanelWidget::new(&items).with_expanded(false);
    let (done, _, total) = widget.counts();
    let line = widget.build_collapsed_line(done, total);
    let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
    assert!(
        text.contains("Working..."),
        "Should show fallback 'Working...' when no InProgress item, got: {text}"
    );
}

#[test]
fn test_collapsed_after_resume_shows_active_form() {
    let items = vec![
        TodoDisplayItem {
            id: 1,
            title: "Done".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
        TodoDisplayItem {
            id: 2,
            title: "Build project".into(),
            status: TodoDisplayStatus::InProgress,
            active_form: Some("Building project".into()),
        },
    ];
    let widget = TodoPanelWidget::new(&items)
        .with_expanded(false)
        .with_spinner_tick(5);
    let (done, _, total) = widget.counts();
    let line = widget.build_collapsed_line(done, total);
    let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
    assert!(
        text.contains("Building project"),
        "Collapsed mode should show active_form after resume, got: {text}"
    );
    assert!(
        text.contains("(1/2)"),
        "Should show progress count, got: {text}"
    );
}

#[test]
fn test_panel_height_lifecycle() {
    // Empty — panel height governed by todo_panel_height() helper (returns 0)
    assert_eq!(todo_panel_height(0, true), 0);

    // Items added — panel visible
    let items = vec![
        TodoDisplayItem {
            id: 1,
            title: "A".into(),
            status: TodoDisplayStatus::Pending,
            active_form: None,
        },
        TodoDisplayItem {
            id: 2,
            title: "B".into(),
            status: TodoDisplayStatus::Pending,
            active_form: None,
        },
    ];
    let w = TodoPanelWidget::new(&items);
    assert_eq!(w.required_height(), 4); // 2 items + 2 borders

    // All done — panel hides
    let done_items = vec![
        TodoDisplayItem {
            id: 1,
            title: "A".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
        TodoDisplayItem {
            id: 2,
            title: "B".into(),
            status: TodoDisplayStatus::Completed,
            active_form: None,
        },
    ];
    let w = TodoPanelWidget::new(&done_items);
    assert_eq!(w.required_height(), 0);

    // New pending items — panel visible again
    let new_items = vec![TodoDisplayItem {
        id: 3,
        title: "C".into(),
        status: TodoDisplayStatus::Pending,
        active_form: None,
    }];
    let w = TodoPanelWidget::new(&new_items);
    assert_eq!(w.required_height(), 3); // 1 item + 2 borders
}

#[test]
fn test_max_items_height_cap() {
    let items: Vec<TodoDisplayItem> = (1..=15)
        .map(|i| TodoDisplayItem {
            id: i,
            title: format!("Task {i}"),
            status: TodoDisplayStatus::Pending,
            active_form: None,
        })
        .collect();
    let w = TodoPanelWidget::new(&items);
    assert_eq!(
        w.required_height(),
        12,
        "15 items should be capped at 12 rows"
    );
}

#[test]
fn test_single_item_all_statuses() {
    for (status, expected_char) in [
        (TodoDisplayStatus::Pending, '\u{25CB}'),
        (TodoDisplayStatus::InProgress, SPINNER_FRAMES[0]),
        (TodoDisplayStatus::Completed, '\u{2714}'),
    ] {
        let items = vec![TodoDisplayItem {
            id: 1,
            title: "Solo".into(),
            status,
            active_form: None,
        }];
        let widget = TodoPanelWidget::new(&items).with_spinner_tick(0);
        let (d, ip, t) = widget.counts();
        let lines = widget.build_lines(d, ip, t);
        let text: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            text.contains(expected_char),
            "Status {status:?} should show '{expected_char}', got: {text}"
        );
    }
}

#[test]
fn test_status_transition_rendering() {
    // Simulate item going through Pending → InProgress → Completed
    let make = |status: TodoDisplayStatus| -> Vec<TodoDisplayItem> {
        vec![TodoDisplayItem {
            id: 1,
            title: "Evolving task".into(),
            status,
            active_form: Some("Evolving".into()),
        }]
    };

    // Pending phase
    let items = make(TodoDisplayStatus::Pending);
    let w = TodoPanelWidget::new(&items).with_spinner_tick(0);
    let (d, ip, t) = w.counts();
    let lines = w.build_lines(d, ip, t);
    let text: String = lines[0]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(text.contains('\u{25CB}'), "Pending phase should show ○");

    // InProgress phase
    let items = make(TodoDisplayStatus::InProgress);
    let w = TodoPanelWidget::new(&items).with_spinner_tick(2);
    let (d, ip, t) = w.counts();
    let lines = w.build_lines(d, ip, t);
    let text: String = lines[0]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(
        text.contains(SPINNER_FRAMES[2]),
        "InProgress phase should show spinner"
    );

    // Completed phase
    let items = make(TodoDisplayStatus::Completed);
    let w = TodoPanelWidget::new(&items).with_spinner_tick(0);
    let (d, ip, t) = w.counts();
    let lines = w.build_lines(d, ip, t);
    let text: String = lines[0]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(text.contains('\u{2714}'), "Completed phase should show ✔");
}

// --- Integration-level lifecycle tests ---

#[test]
fn test_interrupt_resume_display_flow() {
    // Simulate the full interrupt→resume flow at the display level
    use opendev_runtime::todo::TodoManager;

    let mut mgr = TodoManager::from_steps(&["Task A".into(), "Task B".into(), "Task C".into()]);
    mgr.complete(1);
    mgr.start(2);

    // Verify pre-interrupt state
    let display: Vec<TodoDisplayItem> = mgr
        .all()
        .iter()
        .map(|item| TodoDisplayItem {
            id: item.id,
            title: item.title.clone(),
            status: match item.status {
                opendev_runtime::TodoStatus::Pending => TodoDisplayStatus::Pending,
                opendev_runtime::TodoStatus::InProgress => TodoDisplayStatus::InProgress,
                opendev_runtime::TodoStatus::Completed => TodoDisplayStatus::Completed,
            },
            active_form: None,
        })
        .collect();
    assert_eq!(display[1].status, TodoDisplayStatus::InProgress);

    // Simulate interrupt: reset_stuck_todos + sync
    mgr.reset_stuck_todos();
    let display: Vec<TodoDisplayItem> = mgr
        .all()
        .iter()
        .map(|item| TodoDisplayItem {
            id: item.id,
            title: item.title.clone(),
            status: match item.status {
                opendev_runtime::TodoStatus::Pending => TodoDisplayStatus::Pending,
                opendev_runtime::TodoStatus::InProgress => TodoDisplayStatus::InProgress,
                opendev_runtime::TodoStatus::Completed => TodoDisplayStatus::Completed,
            },
            active_form: None,
        })
        .collect();
    assert_eq!(
        display[1].status,
        TodoDisplayStatus::Pending,
        "After interrupt, item should be Pending"
    );
    assert!(
        display
            .iter()
            .all(|i| i.status != TodoDisplayStatus::InProgress)
    );

    // Simulate resume: start next pending + sync
    if let Some(next) = mgr.next_pending() {
        let id = next.id;
        mgr.start(id);
    }
    let display: Vec<TodoDisplayItem> = mgr
        .all()
        .iter()
        .map(|item| TodoDisplayItem {
            id: item.id,
            title: item.title.clone(),
            status: match item.status {
                opendev_runtime::TodoStatus::Pending => TodoDisplayStatus::Pending,
                opendev_runtime::TodoStatus::InProgress => TodoDisplayStatus::InProgress,
                opendev_runtime::TodoStatus::Completed => TodoDisplayStatus::Completed,
            },
            active_form: None,
        })
        .collect();

    // Item 2 should be InProgress again (it's the first pending after item 1 which is completed)
    assert_eq!(
        display[1].status,
        TodoDisplayStatus::InProgress,
        "After resume, item should be InProgress"
    );

    // Widget should render spinner for the resumed item
    let widget = TodoPanelWidget::new(&display).with_spinner_tick(4);
    let (d, ip, t) = widget.counts();
    let lines = widget.build_lines(d, ip, t);
    let text: String = lines[1]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    let expected = SPINNER_FRAMES[4 % SPINNER_FRAMES.len()];
    assert!(
        text.contains(expected),
        "Resumed item should show spinner '{expected}', got: {text}"
    );
}

#[test]
fn test_full_todo_lifecycle() {
    use opendev_runtime::todo::TodoManager;

    let mut mgr = TodoManager::new();
    assert_eq!(mgr.total(), 0);

    // Write todos
    mgr.write_todos(vec![
        (
            "Setup".into(),
            opendev_runtime::TodoStatus::Pending,
            "Setting up".into(),
            Vec::new(),
        ),
        (
            "Build".into(),
            opendev_runtime::TodoStatus::Pending,
            "Building".into(),
            Vec::new(),
        ),
        (
            "Test".into(),
            opendev_runtime::TodoStatus::Pending,
            "Testing".into(),
            Vec::new(),
        ),
    ]);
    assert_eq!(mgr.total(), 3);

    let to_display = |mgr: &TodoManager| -> Vec<TodoDisplayItem> {
        mgr.all()
            .iter()
            .map(|item| TodoDisplayItem {
                id: item.id,
                title: item.title.clone(),
                status: match item.status {
                    opendev_runtime::TodoStatus::Pending => TodoDisplayStatus::Pending,
                    opendev_runtime::TodoStatus::InProgress => TodoDisplayStatus::InProgress,
                    opendev_runtime::TodoStatus::Completed => TodoDisplayStatus::Completed,
                },
                active_form: if item.active_form.is_empty() {
                    None
                } else {
                    Some(item.active_form.clone())
                },
            })
            .collect()
    };

    // Start first
    mgr.start(1);
    let display = to_display(&mgr);
    assert_eq!(display[0].status, TodoDisplayStatus::InProgress);
    let w = TodoPanelWidget::new(&display);
    assert!(w.required_height() > 0, "Panel should be visible");

    // Complete first, start second
    mgr.complete(1);
    mgr.start(2);
    let display = to_display(&mgr);
    assert_eq!(display[0].status, TodoDisplayStatus::Completed);
    assert_eq!(display[1].status, TodoDisplayStatus::InProgress);

    // Complete all
    mgr.complete(2);
    mgr.complete(3);
    let display = to_display(&mgr);
    let w = TodoPanelWidget::new(&display);
    assert_eq!(w.required_height(), 0, "Panel should hide when all done");
}

#[test]
fn test_cancel_and_recreate() {
    use opendev_runtime::todo::TodoManager;

    let mut mgr = TodoManager::from_steps(&["Old A".into(), "Old B".into()]);
    mgr.start(1);

    // Interrupt
    mgr.reset_stuck_todos();

    // Recreate with entirely new todos
    mgr.write_todos(vec![
        (
            "New X".into(),
            opendev_runtime::TodoStatus::Pending,
            String::new(),
            Vec::new(),
        ),
        (
            "New Y".into(),
            opendev_runtime::TodoStatus::InProgress,
            "Doing Y".into(),
            Vec::new(),
        ),
        (
            "New Z".into(),
            opendev_runtime::TodoStatus::Pending,
            String::new(),
            Vec::new(),
        ),
    ]);

    let display: Vec<TodoDisplayItem> = mgr
        .all()
        .iter()
        .map(|item| TodoDisplayItem {
            id: item.id,
            title: item.title.clone(),
            status: match item.status {
                opendev_runtime::TodoStatus::Pending => TodoDisplayStatus::Pending,
                opendev_runtime::TodoStatus::InProgress => TodoDisplayStatus::InProgress,
                opendev_runtime::TodoStatus::Completed => TodoDisplayStatus::Completed,
            },
            active_form: if item.active_form.is_empty() {
                None
            } else {
                Some(item.active_form.clone())
            },
        })
        .collect();

    assert_eq!(display.len(), 3);
    assert_eq!(display[0].title, "New X");
    assert_eq!(display[1].status, TodoDisplayStatus::InProgress);
    assert_eq!(display[1].active_form.as_deref(), Some("Doing Y"));
    assert_eq!(display[2].status, TodoDisplayStatus::Pending);

    // Widget should render properly
    let w = TodoPanelWidget::new(&display).with_spinner_tick(0);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 7));
    w.render(Rect::new(0, 0, 80, 7), &mut buf); // should not panic
}
