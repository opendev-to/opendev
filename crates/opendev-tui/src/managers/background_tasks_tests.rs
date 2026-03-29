use super::*;

fn make_mgr() -> BackgroundTaskManager {
    BackgroundTaskManager {
        tasks: HashMap::new(),
        handles: HashMap::new(),
        output_dir: PathBuf::from("/tmp/opendev-test/tasks"),
        listeners: Vec::new(),
    }
}

#[test]
fn test_new_empty() {
    let mgr = make_mgr();
    assert!(mgr.is_empty());
    assert_eq!(mgr.len(), 0);
    assert!(mgr.active_tasks().is_empty());
    assert_eq!(mgr.running_count(), 0);
}

#[test]
fn test_add_and_get() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "Build project".into());
    assert_eq!(mgr.len(), 1);

    let task = mgr.get_task("t1").unwrap();
    assert_eq!(task.description, "Build project");
    assert_eq!(task.status, "running");
    assert_eq!(task.state, TaskState::Running);
    assert!(task.is_running());
}

#[test]
fn test_update_task() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "Compiling".into());

    assert!(mgr.update_task("t1", "completed".into()));
    let task = mgr.get_task("t1").unwrap();
    assert_eq!(task.status, "completed");
    assert_eq!(task.state, TaskState::Completed);
    assert!(!task.is_running());

    // No longer active
    assert!(mgr.active_tasks().is_empty());

    // Non-existent task
    assert!(!mgr.update_task("nope", "failed".into()));
}

#[test]
fn test_update_task_failed() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "Build".into());
    mgr.update_task("t1", "failed".into());
    assert_eq!(mgr.get_task("t1").unwrap().state, TaskState::Failed);
}

#[test]
fn test_update_task_killed() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "Build".into());
    mgr.update_task("t1", "killed".into());
    assert_eq!(mgr.get_task("t1").unwrap().state, TaskState::Killed);
}

#[test]
fn test_remove_task() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "Running tests".into());
    assert!(mgr.remove_task("t1"));
    assert!(mgr.is_empty());
    assert!(!mgr.remove_task("t1"));
}

#[test]
fn test_active_tasks() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "Build".into());
    mgr.add_task("t2".into(), "Test".into());
    mgr.update_task("t1", "completed".into());

    let active = mgr.active_tasks();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].0, "t2");
    assert_eq!(mgr.running_count(), 1);
}

#[test]
fn test_all_tasks() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "Build".into());
    mgr.add_task("t2".into(), "Test".into());
    let all = mgr.all_tasks();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_runtime_seconds() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "Build".into());
    let task = mgr.get_task("t1").unwrap();
    assert!(task.runtime_seconds() >= 0.0);
}

#[test]
fn test_task_state_display() {
    assert_eq!(TaskState::Running.to_string(), "running");
    assert_eq!(TaskState::Completed.to_string(), "completed");
    assert_eq!(TaskState::Failed.to_string(), "failed");
    assert_eq!(TaskState::Killed.to_string(), "killed");
}

#[test]
fn test_mark_completed_success() {
    let mut tasks = HashMap::new();
    tasks.insert(
        "t1".to_string(),
        TaskStatus {
            task_id: "t1".to_string(),
            command: "echo hi".to_string(),
            description: "test".to_string(),
            started_at: Instant::now(),
            status: "running".to_string(),
            state: TaskState::Running,
            pid: Some(1234),
            output_file: None,
            exit_code: None,
            error_message: None,
            completed_at: None,
        },
    );

    BackgroundTaskManager::mark_completed(&mut tasks, "t1", Some(0));
    let t = tasks.get("t1").unwrap();
    assert_eq!(t.state, TaskState::Completed);
    assert_eq!(t.exit_code, Some(0));
    assert!(t.completed_at.is_some());
}

#[test]
fn test_mark_completed_failure() {
    let mut tasks = HashMap::new();
    tasks.insert(
        "t1".to_string(),
        TaskStatus {
            task_id: "t1".to_string(),
            command: "false".to_string(),
            description: "test".to_string(),
            started_at: Instant::now(),
            status: "running".to_string(),
            state: TaskState::Running,
            pid: None,
            output_file: None,
            exit_code: None,
            error_message: None,
            completed_at: None,
        },
    );

    BackgroundTaskManager::mark_completed(&mut tasks, "t1", Some(1));
    let t = tasks.get("t1").unwrap();
    assert_eq!(t.state, TaskState::Failed);
    assert_eq!(t.error_message.as_deref(), Some("Exited with code 1"));
}

#[test]
fn test_mark_completed_killed() {
    let mut tasks = HashMap::new();
    tasks.insert(
        "t1".to_string(),
        TaskStatus {
            task_id: "t1".to_string(),
            command: "sleep 100".to_string(),
            description: "test".to_string(),
            started_at: Instant::now(),
            status: "running".to_string(),
            state: TaskState::Running,
            pid: None,
            output_file: None,
            exit_code: None,
            error_message: None,
            completed_at: None,
        },
    );

    BackgroundTaskManager::mark_completed(&mut tasks, "t1", Some(137));
    assert_eq!(tasks.get("t1").unwrap().state, TaskState::Killed);
}

#[test]
fn test_read_output_from_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output_file = tmp.path().join("t1.output");
    std::fs::write(&output_file, "line1\nline2\nline3\nline4\nline5\n").unwrap();

    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "test".into());
    if let Some(task) = mgr.tasks.get_mut("t1") {
        task.output_file = Some(output_file);
    }

    // Read all
    let all = mgr.read_output("t1", 0);
    assert_eq!(all.lines().count(), 5);

    // Tail 2
    let tail = mgr.read_output("t1", 2);
    assert_eq!(tail, "line4\nline5");

    // Tail more than available
    let tail_all = mgr.read_output("t1", 100);
    assert_eq!(tail_all.lines().count(), 5);
}

#[test]
fn test_read_output_nonexistent() {
    let mgr = make_mgr();
    assert_eq!(mgr.read_output("nope", 0), "");
}

#[test]
fn test_read_output_no_file() {
    let mut mgr = make_mgr();
    mgr.add_task("t1".into(), "test".into());
    assert_eq!(mgr.read_output("t1", 0), "");
}

#[test]
fn test_get_output_dir() {
    let dir = BackgroundTaskManager::get_output_dir(Path::new("/Users/test/project"));
    assert!(dir.to_string_lossy().contains("opendev"));
    assert!(dir.to_string_lossy().ends_with("tasks"));
}

#[test]
fn test_listener_notification() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let call_count = Arc::new(AtomicUsize::new(0));
    let counter = call_count.clone();

    let mut mgr = make_mgr();
    mgr.add_listener(Box::new(move |_id, _state| {
        counter.fetch_add(1, Ordering::SeqCst);
    }));

    mgr.notify_listeners("t1", TaskState::Running);
    mgr.notify_listeners("t1", TaskState::Completed);
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[cfg(unix)]
#[tokio::test]
async fn test_register_and_stream_task() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut mgr = BackgroundTaskManager::new(tmp.path());

    let child = tokio::process::Command::new("echo")
        .arg("hello from background")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let task_id = mgr.register_task("echo hello from background", child, "");
    assert_eq!(mgr.len(), 1);
    assert!(mgr.get_task(&task_id).unwrap().is_running());
    assert!(mgr.get_task(&task_id).unwrap().pid.is_some());

    // Give the streaming task time to finish
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Output should have been captured
    let output = mgr.read_output(&task_id, 0);
    assert!(
        output.contains("hello from background"),
        "expected output to contain 'hello from background', got: {output}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_kill_task() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut mgr = BackgroundTaskManager::new(tmp.path());

    let child = tokio::process::Command::new("sleep")
        .arg("60")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let task_id = mgr.register_task("sleep 60", child, "");
    assert!(mgr.get_task(&task_id).unwrap().is_running());

    let killed = mgr.kill_task(&task_id).await;
    assert!(killed);

    let task = mgr.get_task(&task_id).unwrap();
    assert_eq!(task.state, TaskState::Killed);
    assert!(!task.is_running());
}

#[tokio::test]
async fn test_cleanup() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut mgr = BackgroundTaskManager::new(tmp.path());

    let child = tokio::process::Command::new("sleep")
        .arg("60")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    mgr.register_task("sleep 60", child, "");
    assert_eq!(mgr.running_count(), 1);

    mgr.cleanup().await;
    assert_eq!(mgr.running_count(), 0);
}
