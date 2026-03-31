use super::*;

fn make_task(id: &str) -> TaskInfo {
    TaskInfo {
        task_id: id.to_string(),
        agent_type: "Explore".to_string(),
        description: "test task".to_string(),
        query: "do something".to_string(),
        session_id: "sess-1".to_string(),
        created_at_ms: now_ms(),
        ..Default::default()
    }
}

#[test]
fn test_create_task_and_retrieve() {
    let tm = TaskManager::new(5);
    let id = tm.create_task(make_task("t1"));
    assert_eq!(id, "t1");

    let info = tm.get("t1").unwrap();
    assert_eq!(info.state, TaskState::Pending);
    assert_eq!(info.agent_type, "Explore");
}

#[test]
fn test_lifecycle_pending_running_completed() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));

    tm.start_task("t1");
    assert_eq!(tm.get("t1").unwrap().state, TaskState::Running);
    assert!(tm.get("t1").unwrap().started_at_ms.is_some());

    tm.complete_task("t1", true, "done", "full result");
    let info = tm.get("t1").unwrap();
    assert_eq!(info.state, TaskState::Completed);
    assert!(info.completed_at_ms.is_some());
    assert_eq!(info.result_summary.as_deref(), Some("done"));
    assert_eq!(info.full_result.as_deref(), Some("full result"));
}

#[test]
fn test_lifecycle_pending_running_failed() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");
    tm.fail_task("t1", "oops");

    let info = tm.get("t1").unwrap();
    assert_eq!(info.state, TaskState::Failed);
    assert_eq!(info.result_summary.as_deref(), Some("oops"));
}

#[test]
fn test_lifecycle_running_killed() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");
    tm.kill_task("t1");

    assert_eq!(tm.get("t1").unwrap().state, TaskState::Killed);
}

#[test]
fn test_kill_already_completed_is_noop() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");
    tm.complete_task("t1", true, "ok", "");

    tm.kill_task("t1"); // should be a no-op
    assert_eq!(tm.get("t1").unwrap().state, TaskState::Completed);
}

#[test]
fn test_kill_already_killed_is_noop() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");
    tm.kill_task("t1");
    tm.kill_task("t1"); // second call is no-op
    assert_eq!(tm.get("t1").unwrap().state, TaskState::Killed);
}

#[test]
fn test_complete_already_completed_is_noop() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");
    tm.complete_task("t1", true, "first", "");
    tm.complete_task("t1", false, "second", ""); // no-op
    assert_eq!(tm.get("t1").unwrap().state, TaskState::Completed);
    assert_eq!(
        tm.get("t1").unwrap().result_summary.as_deref(),
        Some("first")
    );
}

#[test]
fn test_mark_notified_returns_true_once() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    assert!(tm.mark_notified("t1"));
}

#[test]
fn test_mark_notified_returns_false_on_second_call() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    assert!(tm.mark_notified("t1"));
    assert!(!tm.mark_notified("t1"));
}

#[test]
fn test_mark_notified_nonexistent_task() {
    let tm = TaskManager::new(5);
    assert!(!tm.mark_notified("nonexistent"));
}

#[test]
fn test_evict_requires_terminal_and_notified() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");

    // Running task cannot be evicted
    tm.set_evict_after("t1", 0);
    assert!(!tm.try_evict("t1"));

    // Complete it
    tm.complete_task("t1", true, "ok", "");

    // Terminal but not notified
    assert!(!tm.try_evict("t1"));

    // Notify it
    tm.mark_notified("t1");

    // Now evictable (evict_after was set by complete_task)
    assert!(tm.try_evict("t1"));
    assert!(tm.get("t1").is_none());
}

#[test]
fn test_evict_blocked_by_retain() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");
    tm.complete_task("t1", true, "ok", "");
    tm.mark_notified("t1");
    tm.set_retain("t1", true);
    tm.set_evict_after("t1", 0);

    assert!(!tm.try_evict("t1")); // blocked by retain
}

#[test]
fn test_evict_blocked_before_grace_period() {
    let tm = TaskManager::new(5);
    let mut task = make_task("t1");
    task.state = TaskState::Completed;
    task.notified = true;
    task.evict_after_ms = Some(now_ms() + 60_000); // 60s from now
    tm.create_task(task);

    assert!(!tm.try_evict("t1")); // not past deadline yet
}

#[test]
fn test_evict_succeeds_after_grace_period() {
    let tm = TaskManager::new(5);
    let mut task = make_task("t1");
    task.state = TaskState::Completed;
    task.notified = true;
    task.evict_after_ms = Some(0); // already past
    tm.create_task(task);

    assert!(tm.try_evict("t1"));
    assert!(tm.get("t1").is_none());
}

#[test]
fn test_running_count() {
    let tm = TaskManager::new(5);
    assert_eq!(tm.running_count(), 0);

    tm.create_task(make_task("t1"));
    tm.start_task("t1");
    assert_eq!(tm.running_count(), 1);

    tm.create_task(make_task("t2"));
    tm.start_task("t2");
    assert_eq!(tm.running_count(), 2);

    tm.complete_task("t1", true, "ok", "");
    assert_eq!(tm.running_count(), 1);
}

#[test]
fn test_can_accept() {
    let tm = TaskManager::new(2);
    assert!(tm.can_accept());

    tm.create_task(make_task("t1"));
    tm.start_task("t1");
    assert!(tm.can_accept()); // 1 < 2

    tm.create_task(make_task("t2"));
    tm.start_task("t2");
    assert!(!tm.can_accept()); // 2 >= 2
}

#[test]
fn test_activity_log_capped_at_max() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));

    for i in 0..MAX_ACTIVITY_LOG + 50 {
        tm.push_activity("t1", format!("line {i}"));
    }

    let info = tm.get("t1").unwrap();
    assert_eq!(info.activity_log.len(), MAX_ACTIVITY_LOG);
    // Oldest entries were dropped
    assert!(info.activity_log[0].contains("50"));
}

#[test]
fn test_drain_messages_clears_queue() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.queue_message(
        "t1",
        PendingMessage {
            from_agent: "leader".into(),
            content: "hello".into(),
            timestamp_ms: now_ms(),
        },
    );
    tm.queue_message(
        "t1",
        PendingMessage {
            from_agent: "leader".into(),
            content: "world".into(),
            timestamp_ms: now_ms(),
        },
    );

    let msgs = tm.drain_messages("t1");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].content, "hello");
    assert_eq!(msgs[1].content, "world");

    // Queue is now empty
    assert!(tm.drain_messages("t1").is_empty());
}

#[test]
fn test_set_retain_blocks_eviction() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");
    tm.complete_task("t1", true, "ok", "");
    tm.mark_notified("t1");

    tm.set_retain("t1", true);
    assert!(!tm.try_evict("t1"));

    tm.set_retain("t1", false);
    // set_retain(false) on terminal task sets evict_after_ms
    assert!(tm.get("t1").unwrap().evict_after_ms.is_some());
}

#[test]
fn test_background_task() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    assert!(!tm.get("t1").unwrap().is_backgrounded);

    tm.background_task("t1");
    assert!(tm.get("t1").unwrap().is_backgrounded);
}

#[test]
fn test_update_progress() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");

    let activity = ToolActivity {
        tool_name: "read_file".into(),
        description: "Reading src/main.rs".into(),
        is_search: false,
        is_read: true,
        started_at_ms: now_ms(),
        finished: true,
        success: true,
    };
    tm.update_progress("t1", "read_file", Some(activity), 100, 50);

    let info = tm.get("t1").unwrap();
    assert_eq!(info.tool_call_count, 1);
    assert_eq!(info.input_tokens, 100);
    assert_eq!(info.output_tokens, 50);
    assert_eq!(info.current_tool.as_deref(), Some("read_file"));
    assert_eq!(info.recent_activities.len(), 1);
    assert!(info.last_activity.is_some());
}

#[test]
fn test_recent_activities_capped() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");

    for i in 0..MAX_RECENT_ACTIVITIES + 3 {
        let activity = ToolActivity {
            tool_name: format!("tool_{i}"),
            description: format!("desc {i}"),
            is_search: false,
            is_read: false,
            started_at_ms: now_ms(),
            finished: true,
            success: true,
        };
        tm.update_progress("t1", &format!("tool_{i}"), Some(activity), 0, 0);
    }

    let info = tm.get("t1").unwrap();
    assert_eq!(info.recent_activities.len(), MAX_RECENT_ACTIVITIES);
    // Oldest entries were dropped
    assert!(info.recent_activities[0].tool_name.contains('3'));
}

#[test]
fn test_kill_cancels_interrupt_token() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.start_task("t1");

    let token = InterruptToken::new();
    let token_clone = token.clone();
    tm.set_interrupt_token("t1", token);

    assert!(!token_clone.is_requested());
    tm.kill_task("t1");
    assert!(token_clone.is_requested());
}

#[test]
fn test_list_returns_all_tasks() {
    let tm = TaskManager::new(5);
    tm.create_task(make_task("t1"));
    tm.create_task(make_task("t2"));
    tm.create_task(make_task("t3"));

    assert_eq!(tm.list().len(), 3);
}

#[test]
fn test_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let tm = Arc::new(TaskManager::new(100));

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let tm = Arc::clone(&tm);
            thread::spawn(move || {
                let id = format!("t{i}");
                tm.create_task(make_task(&id));
                tm.start_task(&id);
                tm.push_activity(&id, format!("activity from thread {i}"));
                tm.update_progress(&id, "tool", None, 10, 5);
                tm.complete_task(&id, true, "ok", "");
                tm.mark_notified(&id);
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(tm.list().len(), 10);
    assert_eq!(tm.running_count(), 0);
}

#[test]
fn test_get_nonexistent_returns_none() {
    let tm = TaskManager::new(5);
    assert!(tm.get("nonexistent").is_none());
}

#[test]
fn test_start_task_nonexistent_is_noop() {
    let tm = TaskManager::new(5);
    tm.start_task("nonexistent"); // should not panic
}

#[test]
fn test_task_state_display() {
    assert_eq!(TaskState::Pending.to_string(), "pending");
    assert_eq!(TaskState::Running.to_string(), "running");
    assert_eq!(TaskState::Completed.to_string(), "completed");
    assert_eq!(TaskState::Failed.to_string(), "failed");
    assert_eq!(TaskState::Killed.to_string(), "killed");
}

#[test]
fn test_task_state_is_terminal() {
    assert!(!TaskState::Pending.is_terminal());
    assert!(!TaskState::Running.is_terminal());
    assert!(TaskState::Completed.is_terminal());
    assert!(TaskState::Failed.is_terminal());
    assert!(TaskState::Killed.is_terminal());
}

#[tokio::test]
async fn test_event_sender() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tm = TaskManager::new(5).with_event_sender(tx);

    tm.create_task(make_task("t1"));
    tm.start_task("t1");
    tm.complete_task("t1", true, "ok", "");

    // Should receive 3 StateChanged events
    let mut events = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        events.push(evt);
    }
    assert_eq!(events.len(), 3);
    assert!(matches!(
        &events[0],
        TaskManagerEvent::StateChanged {
            new: TaskState::Pending,
            ..
        }
    ));
    assert!(matches!(
        &events[1],
        TaskManagerEvent::StateChanged {
            new: TaskState::Running,
            ..
        }
    ));
    assert!(matches!(
        &events[2],
        TaskManagerEvent::StateChanged {
            new: TaskState::Completed,
            ..
        }
    ));
}
