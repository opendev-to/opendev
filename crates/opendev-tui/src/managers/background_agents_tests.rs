use super::*;

fn make_mgr() -> BackgroundAgentManager {
    BackgroundAgentManager::new()
}

#[test]
fn test_new_empty() {
    let mgr = make_mgr();
    assert!(mgr.is_empty());
    assert_eq!(mgr.len(), 0);
    assert_eq!(mgr.running_count(), 0);
    assert!(mgr.can_accept());
}

#[test]
fn test_add_and_get() {
    let mut mgr = make_mgr();
    mgr.add_task(
        "abc1234".into(),
        "fix the bug".into(),
        "session-1".into(),
        InterruptToken::new(),
    );
    assert_eq!(mgr.len(), 1);
    let task = mgr.get_task("abc1234").unwrap();
    assert_eq!(task.query, "fix the bug");
    assert!(task.is_running());
}

#[test]
fn test_mark_completed() {
    let mut mgr = make_mgr();
    mgr.add_task(
        "t1".into(),
        "query".into(),
        "s1".into(),
        InterruptToken::new(),
    );
    mgr.mark_completed("t1", true, "Done successfully".into(), 5, 0.01);
    let task = mgr.get_task("t1").unwrap();
    assert_eq!(task.state, BackgroundAgentState::Completed);
    assert_eq!(task.tool_call_count, 5);
    assert!(!task.is_running());
}

#[test]
fn test_kill_task() {
    let mut mgr = make_mgr();
    let token = InterruptToken::new();
    let token_clone = token.clone();
    mgr.add_task("t1".into(), "query".into(), "s1".into(), token);
    assert!(mgr.kill_task("t1"));
    assert_eq!(
        mgr.get_task("t1").unwrap().state,
        BackgroundAgentState::Killed
    );
    assert!(token_clone.is_requested());
}

#[test]
fn test_kill_nonexistent() {
    let mut mgr = make_mgr();
    assert!(!mgr.kill_task("nope"));
}

#[test]
fn test_max_concurrent() {
    let mut mgr = make_mgr();
    mgr.max_concurrent = 2;
    for i in 0..2 {
        mgr.add_task(
            format!("t{i}"),
            "q".into(),
            "s".into(),
            InterruptToken::new(),
        );
    }
    assert!(!mgr.can_accept());
    mgr.mark_completed("t0", true, "done".into(), 0, 0.0);
    assert!(mgr.can_accept());
}

#[test]
fn test_all_tasks_sorted() {
    let mut mgr = make_mgr();
    mgr.add_task(
        "t1".into(),
        "first".into(),
        "s".into(),
        InterruptToken::new(),
    );
    mgr.add_task(
        "t2".into(),
        "second".into(),
        "s".into(),
        InterruptToken::new(),
    );
    let tasks = mgr.all_tasks();
    assert_eq!(tasks.len(), 2);
    // Most recent first
    assert_eq!(tasks[0].task_id, "t2");
}

#[test]
fn test_update_progress() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "q".into(), "s".into(), InterruptToken::new());
    mgr.update_progress("t1", "bash".into(), 3);
    let task = mgr.get_task("t1").unwrap();
    assert_eq!(task.current_tool.as_deref(), Some("bash"));
    assert_eq!(task.tool_call_count, 3);
}

#[test]
fn test_mark_completed_preserves_killed() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "q".into(), "s".into(), InterruptToken::new());
    mgr.kill_task("t1");
    assert_eq!(
        mgr.get_task("t1").unwrap().state,
        BackgroundAgentState::Killed
    );
    // mark_completed should NOT overwrite Killed → Failed
    mgr.mark_completed("t1", false, "interrupted".into(), 2, 0.0);
    assert_eq!(
        mgr.get_task("t1").unwrap().state,
        BackgroundAgentState::Killed
    );
}

#[test]
fn test_pending_spawn_count() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "q".into(), "s".into(), InterruptToken::new());
    assert_eq!(mgr.get_task("t1").unwrap().pending_spawn_count, 0);
    mgr.increment_pending_spawn("t1");
    mgr.increment_pending_spawn("t1");
    assert_eq!(mgr.get_task("t1").unwrap().pending_spawn_count, 2);
    mgr.decrement_pending_spawn("t1");
    assert_eq!(mgr.get_task("t1").unwrap().pending_spawn_count, 1);
    // saturating_sub prevents underflow
    mgr.decrement_pending_spawn("t1");
    mgr.decrement_pending_spawn("t1");
    assert_eq!(mgr.get_task("t1").unwrap().pending_spawn_count, 0);
}

#[test]
fn test_state_display() {
    assert_eq!(BackgroundAgentState::Running.to_string(), "running");
    assert_eq!(BackgroundAgentState::Completed.to_string(), "completed");
    assert_eq!(BackgroundAgentState::Failed.to_string(), "failed");
    assert_eq!(BackgroundAgentState::Killed.to_string(), "killed");
}
