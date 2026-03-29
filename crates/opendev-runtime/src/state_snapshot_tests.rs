use super::*;
use tempfile::TempDir;

#[test]
fn test_snapshot_new() {
    let snap = AppStateSnapshot::new("sess-1", "/project");
    assert_eq!(snap.session_id, "sess-1");
    assert_eq!(snap.project_dir, "/project");
    assert_eq!(snap.message_count, 0);
    assert!(!snap.completed);
    assert!(snap.snapshot_timestamp_ms > 0);
}

#[test]
fn test_record_tool_result() {
    let mut snap = AppStateSnapshot::new("s1", "/p");
    for i in 0..5 {
        snap.record_tool_result(
            ToolResultEntry {
                tool_name: format!("tool_{i}"),
                call_id: format!("c{i}"),
                output_preview: "ok".into(),
                success: true,
            },
            3,
        );
    }
    assert_eq!(snap.last_tool_results.len(), 3);
    // Should keep the last 3 (tool_2, tool_3, tool_4).
    assert_eq!(snap.last_tool_results[0].tool_name, "tool_2");
    assert_eq!(snap.last_tool_results[2].tool_name, "tool_4");
}

#[test]
fn test_mark_completed() {
    let mut snap = AppStateSnapshot::new("s1", "/p");
    assert!(!snap.completed);
    snap.mark_completed();
    assert!(snap.completed);
}

#[test]
fn test_save_and_load() {
    let tmp = TempDir::new().unwrap();
    let persistence = SnapshotPersistence::with_dir(tmp.path());

    let mut snap = AppStateSnapshot::new("session-abc", "/my/project");
    snap.message_count = 42;
    snap.cost_usd = 0.15;

    persistence.save(&snap).unwrap();

    let loaded = persistence.load("session-abc").unwrap();
    assert_eq!(loaded.session_id, "session-abc");
    assert_eq!(loaded.message_count, 42);
    assert!((loaded.cost_usd - 0.15).abs() < f64::EPSILON);
    assert!(!loaded.completed);
}

#[test]
fn test_load_nonexistent() {
    let tmp = TempDir::new().unwrap();
    let persistence = SnapshotPersistence::with_dir(tmp.path());
    assert!(persistence.load("nope").is_none());
}

#[test]
fn test_find_incomplete_sessions() {
    let tmp = TempDir::new().unwrap();
    let persistence = SnapshotPersistence::with_dir(tmp.path());

    // Save a completed session.
    let mut snap1 = AppStateSnapshot::new("s1", "/p");
    snap1.mark_completed();
    persistence.save(&snap1).unwrap();

    // Save an incomplete session.
    let snap2 = AppStateSnapshot::new("s2", "/p");
    persistence.save(&snap2).unwrap();

    let incomplete = persistence.find_incomplete_sessions();
    assert_eq!(incomplete.len(), 1);
    assert_eq!(incomplete[0].session_id, "s2");
}

#[test]
fn test_remove_snapshot() {
    let tmp = TempDir::new().unwrap();
    let persistence = SnapshotPersistence::with_dir(tmp.path());

    let snap = AppStateSnapshot::new("s1", "/p");
    persistence.save(&snap).unwrap();
    assert!(persistence.load("s1").is_some());

    assert!(persistence.remove("s1"));
    assert!(persistence.load("s1").is_none());
}

#[test]
fn test_remove_nonexistent() {
    let tmp = TempDir::new().unwrap();
    let persistence = SnapshotPersistence::with_dir(tmp.path());
    assert!(!persistence.remove("nonexistent"));
}

#[test]
fn test_snapshot_serialization_roundtrip() {
    let mut snap = AppStateSnapshot::new("s1", "/project/dir");
    snap.message_count = 10;
    snap.cost_usd = 1.23;
    snap.record_tool_result(
        ToolResultEntry {
            tool_name: "bash".into(),
            call_id: "c1".into(),
            output_preview: "hello world".into(),
            success: true,
        },
        10,
    );

    let json = serde_json::to_string(&snap).unwrap();
    let deserialized: AppStateSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snap, deserialized);
}

#[test]
fn test_snapshot_path() {
    let tmp = TempDir::new().unwrap();
    let persistence = SnapshotPersistence::with_dir(tmp.path());
    let path = persistence.snapshot_path("my-session");
    assert!(path.to_string_lossy().contains("my-session"));
    assert!(path.to_string_lossy().ends_with(".json"));
}

#[test]
fn test_cleanup_old_removes_expired() {
    let tmp = TempDir::new().unwrap();
    let persistence = SnapshotPersistence::with_dir(tmp.path());

    // Save a snapshot with an old timestamp.
    let mut snap = AppStateSnapshot::new("old-session", "/p");
    snap.snapshot_timestamp_ms = 1000; // Very old.
    persistence.save(&snap).unwrap();

    // Save a recent snapshot.
    let snap2 = AppStateSnapshot::new("new-session", "/p");
    persistence.save(&snap2).unwrap();

    let removed = persistence.cleanup_old(Duration::from_secs(1));
    assert_eq!(removed, 1);

    assert!(persistence.load("old-session").is_none());
    assert!(persistence.load("new-session").is_some());
}
