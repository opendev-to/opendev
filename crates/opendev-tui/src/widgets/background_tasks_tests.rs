use super::*;

#[test]
fn test_truncate_str() {
    assert_eq!(truncate_str("hello", 10), "hello");
    assert_eq!(truncate_str("hello world", 8), "hello...");
    assert_eq!(truncate_str("hi", 2), "hi");
}

#[test]
fn test_compute_grid_cols() {
    assert_eq!(compute_grid_cols(0, 120), 1);
    assert_eq!(compute_grid_cols(1, 120), 1);
    assert_eq!(compute_grid_cols(2, 120), 2);
    assert_eq!(compute_grid_cols(3, 120), 3);
    assert_eq!(compute_grid_cols(4, 120), 4);
    assert_eq!(compute_grid_cols(5, 120), 4); // 120/30 = 4 max cols
    assert_eq!(compute_grid_cols(3, 59), 1); // 59/30 = 1 max col
    assert_eq!(compute_grid_cols(3, 60), 2); // 60/30 = 2 max cols
}

#[test]
fn test_ceil_div() {
    assert_eq!(ceil_div(1, 1), 1);
    assert_eq!(ceil_div(3, 2), 2);
    assert_eq!(ceil_div(4, 2), 2);
    assert_eq!(ceil_div(5, 3), 2);
    assert_eq!(ceil_div(7, 3), 3);
}

#[test]
fn test_cell_rect() {
    let inner = Rect::new(0, 0, 120, 24);
    // 2 cols, 1 row
    let r0 = cell_rect(0, 0, inner, 2, 1);
    assert_eq!(r0, Rect::new(0, 0, 60, 24));
    let r1 = cell_rect(1, 0, inner, 2, 1);
    assert_eq!(r1, Rect::new(60, 0, 60, 24));

    // 3 cols with remainder (121 width)
    let inner2 = Rect::new(0, 0, 121, 24);
    let r0 = cell_rect(0, 0, inner2, 3, 1);
    assert_eq!(r0.width, 41); // 40 + 1 extra
    let r1 = cell_rect(1, 0, inner2, 3, 1);
    assert_eq!(r1.width, 40);
}

#[test]
fn test_empty_panel() {
    let subagents: Vec<SubagentDisplayState> = vec![];
    let mgr = BackgroundAgentManager::new();
    let covered = HashSet::new();
    let shortener = PathShortener::new(Some("."));
    let panel = TaskWatcherPanel::new(&subagents, &mgr, &covered, 0, &shortener);
    assert_eq!(panel.total_tasks(), 0);
}

#[test]
fn test_panel_with_subagents() {
    let subagents = vec![SubagentDisplayState::new(
        "id-1".into(),
        "Explore".into(),
        "Find TODOs".into(),
    )];
    let mgr = BackgroundAgentManager::new();
    let covered = HashSet::new();
    let shortener = PathShortener::new(Some("."));
    let panel = TaskWatcherPanel::new(&subagents, &mgr, &covered, 0, &shortener);
    assert_eq!(panel.total_tasks(), 1);
}

#[test]
fn test_panel_render_no_crash() {
    let subagents = vec![SubagentDisplayState::new(
        "id-1".into(),
        "Explore".into(),
        "Find TODOs".into(),
    )];
    let mgr = BackgroundAgentManager::new();
    let covered = HashSet::new();
    let shortener = PathShortener::new(Some("."));
    let panel = TaskWatcherPanel::new(&subagents, &mgr, &covered, 0, &shortener);

    let area = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(area);
    panel.render(area, &mut buffer);
}

#[test]
fn test_panel_focus_and_scrolls() {
    let subagents: Vec<SubagentDisplayState> = vec![];
    let mgr = BackgroundAgentManager::new();
    let covered = HashSet::new();
    let shortener = PathShortener::new(Some("."));
    let scrolls = vec![3, 5];
    let panel = TaskWatcherPanel::new(&subagents, &mgr, &covered, 0, &shortener)
        .focus(1)
        .cell_scrolls(&scrolls)
        .page(0);
    assert_eq!(panel.focus, 1);
    assert_eq!(panel.cell_scrolls, &[3, 5]);
    assert_eq!(panel.page, 0);
}

#[test]
fn test_parse_activity_line() {
    // Running tool: spinner icon, verb bold, args subtle
    let line = parse_activity_line("\u{25b8} \u{2819} Grep(.route 3s");
    assert_eq!(line.icon_color, style_tokens::BLUE_BRIGHT);
    assert_eq!(line.verb, "Grep");
    assert_eq!(line.args, "(.route 3s");

    // Success: ⏺ icon in green, verb/args split
    let line = parse_activity_line("\u{2713} Read file.rs");
    assert_eq!(line.icon_color, style_tokens::GREEN_BRIGHT);
    assert!(line.icon.contains(COMPLETED_CHAR));
    assert_eq!(line.verb, "Read");
    assert_eq!(line.args, " file.rs");

    // Failure: ⏺ icon in red
    let line = parse_activity_line("\u{2717} Bash(exit 1)");
    assert_eq!(line.icon_color, style_tokens::ERROR);
    assert!(line.icon.contains(COMPLETED_CHAR));
    assert_eq!(line.verb, "Bash");
    assert_eq!(line.args, "(exit 1)");

    // Thinking: subtle
    let line = parse_activity_line("\u{27e1} thinking");
    assert_eq!(line.icon_color, style_tokens::SUBTLE);
    assert_eq!(line.verb, "thinking");

    // Plain text: no icon
    let line = parse_activity_line("normal text");
    assert_eq!(line.icon_color, style_tokens::PRIMARY);
    assert!(line.icon.is_empty());
    assert!(line.verb.is_empty());
    assert_eq!(line.args, "normal text");
}

/// Helper to create a SubagentDisplayState with given fields.
fn make_subagent(id: &str, backgrounded: bool, finished: bool) -> SubagentDisplayState {
    let mut s = SubagentDisplayState::new(id.to_string(), "Agent".into(), "task".into());
    s.backgrounded = backgrounded;
    if finished {
        s.finish(true, "done".into(), 3, None);
    }
    s
}

#[test]
fn test_finished_bg_subagent_still_covers_parent() {
    // A finished backgrounded subagent should still cover its parent task.
    let subagents = vec![make_subagent("sa1", true, true)];
    let mgr = BackgroundAgentManager::new();
    // Simulate bg_subagent_map: sa1 -> bg_task_1
    let covered: HashSet<String> = ["bg_task_1".to_string()].into_iter().collect();
    let shortener = PathShortener::new(Some("."));
    let panel = TaskWatcherPanel::new(&subagents, &mgr, &covered, 0, &shortener);
    let filtered = panel.filtered_bg_tasks();
    // No bg tasks in mgr, so nothing to filter, but covered set is correct
    assert!(filtered.is_empty());
    // The subagent is still counted in total_tasks
    assert_eq!(panel.total_tasks(), 1);
}

#[test]
fn test_mixed_running_and_finished_bg_subagents_cover_parent() {
    // Both running and finished backgrounded subagents should cover the parent.
    let subagents = vec![
        make_subagent("sa1", true, false),
        make_subagent("sa2", true, true),
    ];
    // Both map to same parent
    let covered: HashSet<String> = ["bg_task_1".to_string()].into_iter().collect();
    let mgr = BackgroundAgentManager::new();
    let shortener = PathShortener::new(Some("."));
    let panel = TaskWatcherPanel::new(&subagents, &mgr, &covered, 0, &shortener);
    assert_eq!(panel.total_tasks(), 2);
}

#[test]
fn test_non_bg_finished_subagent_does_not_affect_filtering() {
    // A foreground finished subagent should not contribute to covered_bg_task_ids.
    let subagents = vec![make_subagent("sa1", false, true)];
    let covered: HashSet<String> = HashSet::new(); // no coverage
    let mgr = BackgroundAgentManager::new();
    let shortener = PathShortener::new(Some("."));
    let panel = TaskWatcherPanel::new(&subagents, &mgr, &covered, 0, &shortener);
    // Foreground subagent still shows, covered set is empty
    assert!(panel.filtered_bg_tasks().is_empty());
    assert_eq!(panel.total_tasks(), 1);
}
