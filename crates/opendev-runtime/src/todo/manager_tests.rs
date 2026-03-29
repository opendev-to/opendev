use super::*;

#[test]
fn test_todo_manager_basic() {
    let mut mgr = TodoManager::new();
    assert!(!mgr.has_todos());
    assert_eq!(mgr.total(), 0);

    let id1 = mgr.add("First step".to_string());
    let id2 = mgr.add("Second step".to_string());

    assert!(mgr.has_todos());
    assert_eq!(mgr.total(), 2);
    assert_eq!(mgr.pending_count(), 2);
    assert_eq!(mgr.completed_count(), 0);

    assert_eq!(mgr.get(id1).unwrap().title, "First step");
    assert_eq!(mgr.get(id1).unwrap().status, TodoStatus::Pending);

    mgr.start(id1);
    assert_eq!(mgr.get(id1).unwrap().status, TodoStatus::InProgress);
    assert_eq!(mgr.in_progress_count(), 1);

    mgr.complete(id1);
    assert_eq!(mgr.get(id1).unwrap().status, TodoStatus::Completed);
    assert_eq!(mgr.completed_count(), 1);

    assert!(!mgr.all_completed());
    mgr.complete(id2);
    assert!(mgr.all_completed());
}

#[test]
fn test_todo_manager_from_steps() {
    let steps = vec![
        "Set up project".to_string(),
        "Write code".to_string(),
        "Test".to_string(),
    ];
    let mgr = TodoManager::from_steps(&steps);
    assert_eq!(mgr.total(), 3);
    let items = mgr.all();
    assert_eq!(items[0].title, "Set up project");
    assert_eq!(items[1].title, "Write code");
    assert_eq!(items[2].title, "Test");
}

#[test]
fn test_next_pending() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into(), "C".into()]);
    assert_eq!(mgr.next_pending().unwrap().id, 1);

    mgr.complete(1);
    assert_eq!(mgr.next_pending().unwrap().id, 2);

    mgr.complete(2);
    mgr.complete(3);
    assert!(mgr.next_pending().is_none());
}

#[test]
fn test_format_status() {
    let mut mgr = TodoManager::from_steps(&["Step A".into(), "Step B".into()]);
    mgr.complete(1);
    let status = mgr.format_status();
    assert!(status.contains("1/2 done"));
    assert!(status.contains("[done] 1. Step A"));
    assert!(status.contains("[todo] 2. Step B"));
}

#[test]
fn test_remove_and_clear() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into()]);
    assert!(mgr.remove(1));
    assert_eq!(mgr.total(), 1);
    assert!(!mgr.remove(1)); // Already removed

    mgr.clear();
    assert_eq!(mgr.total(), 0);
    assert!(!mgr.has_todos());
}

#[test]
fn test_set_status_nonexistent() {
    let mut mgr = TodoManager::new();
    assert!(!mgr.set_status(999, TodoStatus::Completed));
}

#[test]
fn test_single_active_constraint() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into(), "C".into()]);
    mgr.start(1);
    assert_eq!(mgr.get(1).unwrap().status, TodoStatus::InProgress);

    // Starting another should revert the first
    mgr.start(2);
    assert_eq!(mgr.get(1).unwrap().status, TodoStatus::Pending);
    assert_eq!(mgr.get(2).unwrap().status, TodoStatus::InProgress);
}

#[test]
fn test_add_with_status() {
    let mut mgr = TodoManager::new();
    let id = mgr.add_with_status(
        "Test item".into(),
        TodoStatus::InProgress,
        "Testing...".into(),
        Vec::new(),
    );
    assert_eq!(mgr.get(id).unwrap().status, TodoStatus::InProgress);
    assert_eq!(mgr.get(id).unwrap().active_form, "Testing...");
}

#[test]
fn test_write_todos() {
    let mut mgr = TodoManager::from_steps(&["Old".into()]);
    mgr.write_todos(vec![
        (
            "New A".into(),
            TodoStatus::Pending,
            String::new(),
            Vec::new(),
        ),
        (
            "New B".into(),
            TodoStatus::InProgress,
            "Working on B".into(),
            Vec::new(),
        ),
    ]);
    assert_eq!(mgr.total(), 2);
    assert_eq!(mgr.get(1).unwrap().title, "New A");
    assert_eq!(mgr.get(2).unwrap().active_form, "Working on B");
}

#[test]
fn test_write_todos_with_children() {
    let mut mgr = TodoManager::new();
    mgr.write_todos(vec![
        (
            "Implement auth".into(),
            TodoStatus::Pending,
            "Implementing auth".into(),
            vec![
                SubTodoItem {
                    title: "Add login endpoint".into(),
                },
                SubTodoItem {
                    title: "Add token validation".into(),
                },
            ],
        ),
        (
            "Write tests".into(),
            TodoStatus::Pending,
            "Writing tests".into(),
            vec![SubTodoItem {
                title: "Unit tests".into(),
            }],
        ),
    ]);
    // total() counts only parents
    assert_eq!(mgr.total(), 2);
    assert_eq!(mgr.get(1).unwrap().children.len(), 2);
    assert_eq!(mgr.get(1).unwrap().children[0].title, "Add login endpoint");
    assert_eq!(mgr.get(2).unwrap().children.len(), 1);

    // format_status includes children
    let status = mgr.format_status();
    assert!(status.contains("- Add login endpoint"));
    assert!(status.contains("- Add token validation"));
    assert!(status.contains("- Unit tests"));
}

#[test]
fn test_get_active_todo_message() {
    let mut mgr = TodoManager::new();
    mgr.add_with_status(
        "Task".into(),
        TodoStatus::InProgress,
        "Doing task".into(),
        Vec::new(),
    );
    assert_eq!(
        mgr.get_active_todo_message(),
        Some("Doing task".to_string())
    );
}

#[test]
fn test_has_incomplete_todos() {
    let mut mgr = TodoManager::from_steps(&["A".into()]);
    assert!(mgr.has_incomplete_todos());
    mgr.complete(1);
    assert!(!mgr.has_incomplete_todos());
}

#[test]
fn test_has_work_in_progress() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into()]);
    // All pending — no work started
    assert!(!mgr.has_work_in_progress());

    // Start one — work in progress
    mgr.start(1);
    assert!(mgr.has_work_in_progress());

    // Complete one, other still pending — still has work in progress
    mgr.complete(1);
    assert!(mgr.has_work_in_progress());

    // Complete all — still true (Completed != Pending)
    mgr.complete(2);
    assert!(mgr.has_work_in_progress());
}

#[test]
fn test_format_status_sorted() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into(), "C".into()]);
    mgr.start(2);
    mgr.complete(3);
    let status = mgr.format_status_sorted();
    // "doing" should appear before "todo" and "done"
    let doing_pos = status.find("[doing]").unwrap();
    let todo_pos = status.find("[todo]").unwrap();
    let done_pos = status.find("[done]").unwrap();
    assert!(doing_pos < todo_pos);
    assert!(todo_pos < done_pos);
}

#[test]
fn test_reset_stuck_todos() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into(), "C".into()]);
    mgr.start(1);
    mgr.complete(3);

    // A is "doing", B is "pending", C is "done"
    assert_eq!(mgr.get(1).unwrap().status, TodoStatus::InProgress);
    assert_eq!(mgr.get(2).unwrap().status, TodoStatus::Pending);
    assert_eq!(mgr.get(3).unwrap().status, TodoStatus::Completed);

    let reset_count = mgr.reset_stuck_todos();
    assert_eq!(reset_count, 1);
    // A should be reset to pending
    assert_eq!(mgr.get(1).unwrap().status, TodoStatus::Pending);
    // B stays pending
    assert_eq!(mgr.get(2).unwrap().status, TodoStatus::Pending);
    // C stays done
    assert_eq!(mgr.get(3).unwrap().status, TodoStatus::Completed);
}

#[test]
fn test_reset_stuck_todos_none_doing() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into()]);
    mgr.complete(1);
    let reset_count = mgr.reset_stuck_todos();
    assert_eq!(reset_count, 0);
}

#[test]
fn test_interrupt_resume_cycle() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into(), "C".into()]);
    mgr.start(2);
    assert_eq!(mgr.get(2).unwrap().status, TodoStatus::InProgress);

    // Simulate interrupt
    let reset = mgr.reset_stuck_todos();
    assert_eq!(reset, 1);
    assert_eq!(mgr.get(2).unwrap().status, TodoStatus::Pending);
    assert_eq!(mgr.in_progress_count(), 0);

    // Simulate resume — start same item again
    mgr.start(2);
    assert_eq!(mgr.get(2).unwrap().status, TodoStatus::InProgress);
    assert_eq!(mgr.in_progress_count(), 1);
}

#[test]
fn test_multiple_interrupt_resume_cycles() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into()]);

    for _ in 0..3 {
        mgr.start(1);
        assert_eq!(mgr.get(1).unwrap().status, TodoStatus::InProgress);

        let reset = mgr.reset_stuck_todos();
        assert_eq!(reset, 1);
        assert_eq!(mgr.get(1).unwrap().status, TodoStatus::Pending);
    }

    // Final resume
    mgr.start(1);
    assert_eq!(mgr.get(1).unwrap().status, TodoStatus::InProgress);
    assert_eq!(mgr.pending_count(), 1); // B still pending
}

#[test]
fn test_interrupt_preserves_completed() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into(), "C".into()]);
    mgr.complete(1);
    mgr.start(2);
    // A=done, B=doing, C=pending

    let reset = mgr.reset_stuck_todos();
    assert_eq!(reset, 1);
    assert_eq!(mgr.get(1).unwrap().status, TodoStatus::Completed);
    assert_eq!(mgr.get(2).unwrap().status, TodoStatus::Pending);
    assert_eq!(mgr.get(3).unwrap().status, TodoStatus::Pending);
}

#[test]
fn test_write_todos_after_interrupt() {
    let mut mgr = TodoManager::from_steps(&["Old A".into(), "Old B".into()]);
    mgr.start(1);
    mgr.reset_stuck_todos();

    // Write entirely new todos
    mgr.write_todos(vec![
        (
            "New X".into(),
            TodoStatus::Pending,
            String::new(),
            Vec::new(),
        ),
        (
            "New Y".into(),
            TodoStatus::InProgress,
            "Working on Y".into(),
            Vec::new(),
        ),
    ]);

    assert_eq!(mgr.total(), 2);
    assert_eq!(mgr.get(1).unwrap().title, "New X");
    assert_eq!(mgr.get(2).unwrap().status, TodoStatus::InProgress);
}

#[test]
fn test_clear_after_interrupt() {
    let mut mgr = TodoManager::from_steps(&["A".into(), "B".into()]);
    mgr.start(1);
    mgr.reset_stuck_todos();
    assert_eq!(mgr.total(), 2);

    mgr.clear();
    assert_eq!(mgr.total(), 0);
    assert!(!mgr.has_todos());
}

#[test]
fn test_find_todo_formats() {
    let mgr = TodoManager::from_steps(&["Alpha".into(), "Beta".into(), "Gamma".into()]);

    // Numeric
    assert_eq!(mgr.find_todo("2").unwrap().0, 2);
    // todo-N
    assert_eq!(mgr.find_todo("todo-1").unwrap().0, 1);
    // todo_N
    assert_eq!(mgr.find_todo("todo_3").unwrap().0, 3);
    // Exact title
    assert_eq!(mgr.find_todo("Beta").unwrap().0, 2);
    // Partial title
    assert_eq!(mgr.find_todo("alph").unwrap().0, 1);
    // Not found
    assert!(mgr.find_todo("xyz").is_none());
}
