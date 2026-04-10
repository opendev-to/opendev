use std::fs;

use tempfile::TempDir;

use super::*;

fn make_manager(tmp: &TempDir) -> FileCheckpointManager {
    let working_dir = tmp.path().join("project");
    fs::create_dir_all(&working_dir).unwrap();
    // Override base_dir to use temp dir instead of ~/.opendev/
    let base_dir = tmp.path().join("checkpoints").join("test-session");
    let mut mgr = FileCheckpointManager {
        session_id: "test-session".to_string(),
        working_dir,
        base_dir,
        turns: Vec::new(),
        current_turn: None,
        next_turn_id: 0,
        next_version: HashMap::new(),
    };
    mgr.load_manifest();
    mgr
}

#[test]
fn test_capture_and_undo_round_trip() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = make_manager(&tmp);

    // Create a file with initial content
    let file_path = mgr.working_dir.join("hello.txt");
    fs::write(&file_path, "original content").unwrap();

    // Begin turn and capture before edit
    mgr.begin_turn();
    mgr.capture_file(&file_path).unwrap();

    // Simulate edit
    fs::write(&file_path, "modified content").unwrap();

    // End turn
    let stats = mgr.end_turn_with_stats();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].file_path, "hello.txt");
    assert!(stats[0].additions > 0 || stats[0].deletions > 0);

    // Undo
    let desc = mgr.undo_last_turn().unwrap();
    assert!(desc.contains("hello.txt"));

    // File should be restored
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "original content");
}

#[test]
fn test_undo_new_file_deletes_it() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = make_manager(&tmp);

    let file_path = mgr.working_dir.join("new_file.txt");
    assert!(!file_path.exists());

    // Begin turn, capture non-existent file
    mgr.begin_turn();
    mgr.capture_file(&file_path).unwrap();

    // Simulate tool creating the file
    fs::write(&file_path, "new content").unwrap();

    let stats = mgr.end_turn_with_stats();
    assert_eq!(stats.len(), 1);

    // Undo should delete the file
    let desc = mgr.undo_last_turn().unwrap();
    assert!(desc.contains("new_file.txt"));
    assert!(!file_path.exists());
}

#[test]
fn test_dedup_within_turn() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = make_manager(&tmp);

    let file_path = mgr.working_dir.join("file.txt");
    fs::write(&file_path, "v1").unwrap();

    mgr.begin_turn();
    mgr.capture_file(&file_path).unwrap();
    mgr.capture_file(&file_path).unwrap(); // duplicate — should be skipped

    let turn = mgr.current_turn.as_ref().unwrap();
    assert_eq!(turn.files.len(), 1, "Duplicate capture should be skipped");

    mgr.end_turn_with_stats();
}

#[test]
fn test_no_turns_undo_returns_none() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = make_manager(&tmp);

    assert!(mgr.undo_last_turn().is_none());
}

#[test]
fn test_snapshot_cap_enforcement() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = make_manager(&tmp);

    // Create many turns with files to exceed the cap
    for i in 0..120 {
        let file_path = mgr.working_dir.join(format!("file_{i}.txt"));
        fs::write(&file_path, format!("content {i}")).unwrap();

        mgr.begin_turn();
        mgr.capture_file(&file_path).unwrap();

        fs::write(&file_path, format!("modified {i}")).unwrap();
        mgr.end_turn_with_stats();
    }

    let total: usize = mgr.turns.iter().map(|t| t.files.len()).sum();
    assert!(total <= 100, "Snapshot cap should be enforced: got {total}");
}

#[test]
fn test_diff_stats_computation() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = make_manager(&tmp);

    let file_path = mgr.working_dir.join("code.rs");
    fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

    mgr.begin_turn();
    mgr.capture_file(&file_path).unwrap();

    // Modify: change line2, add line4
    fs::write(&file_path, "line1\nline2_modified\nline3\nline4\n").unwrap();

    let stats = mgr.end_turn_with_stats();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].file_path, "code.rs");
    // Should have: 1 deletion (old line2), 2 additions (new line2 + line4)
    assert!(stats[0].additions > 0);
    assert!(stats[0].deletions > 0);
    assert!(!stats[0].is_binary);
}

#[test]
fn test_manifest_persistence_and_reload() {
    let tmp = TempDir::new().unwrap();
    let working_dir = tmp.path().join("project");
    fs::create_dir_all(&working_dir).unwrap();
    let base_dir = tmp.path().join("checkpoints").join("persist-session");

    // Create manager, do some work
    {
        let mut mgr = FileCheckpointManager {
            session_id: "persist-session".to_string(),
            working_dir: working_dir.clone(),
            base_dir: base_dir.clone(),
            turns: Vec::new(),
            current_turn: None,
            next_turn_id: 0,
            next_version: HashMap::new(),
        };

        let file_path = working_dir.join("persist.txt");
        fs::write(&file_path, "original").unwrap();

        mgr.begin_turn();
        mgr.capture_file(&file_path).unwrap();
        fs::write(&file_path, "modified").unwrap();
        mgr.end_turn_with_stats();

        assert_eq!(mgr.turn_count(), 1);
    }

    // Create new manager from same dir — should reload
    {
        let mut mgr = FileCheckpointManager {
            session_id: "persist-session".to_string(),
            working_dir: working_dir.clone(),
            base_dir: base_dir.clone(),
            turns: Vec::new(),
            current_turn: None,
            next_turn_id: 0,
            next_version: HashMap::new(),
        };
        mgr.load_manifest();

        assert_eq!(mgr.turn_count(), 1);

        // Undo should still work
        let file_path = working_dir.join("persist.txt");
        let desc = mgr.undo_last_turn().unwrap();
        assert!(desc.contains("persist.txt"));
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "original");
    }
}

#[test]
fn test_turn_count() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = make_manager(&tmp);

    assert_eq!(mgr.turn_count(), 0);

    let file = mgr.working_dir.join("f.txt");
    fs::write(&file, "a").unwrap();

    mgr.begin_turn();
    mgr.capture_file(&file).unwrap();
    fs::write(&file, "b").unwrap();
    mgr.end_turn_with_stats();

    assert_eq!(mgr.turn_count(), 1);

    mgr.begin_turn();
    mgr.capture_file(&file).unwrap();
    fs::write(&file, "c").unwrap();
    mgr.end_turn_with_stats();

    assert_eq!(mgr.turn_count(), 2);

    mgr.undo_last_turn();
    assert_eq!(mgr.turn_count(), 1);
}

#[test]
fn test_empty_turn_produces_no_stats() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = make_manager(&tmp);

    mgr.begin_turn();
    let stats = mgr.end_turn_with_stats();
    assert!(stats.is_empty());
    assert_eq!(mgr.turn_count(), 0);
}
