use super::*;

#[test]
fn test_encode_project_id() {
    let id1 = encode_project_id("/Users/foo/project");
    let id2 = encode_project_id("/Users/foo/project");
    assert_eq!(id1, id2); // Deterministic

    let id3 = encode_project_id("/Users/bar/project");
    assert_ne!(id1, id3); // Different paths -> different IDs

    assert_eq!(id1.len(), 16); // Fixed width hex
}

#[test]
fn test_snapshot_manager_new() {
    let mgr = SnapshotManager::new("/tmp/test-project");
    assert_eq!(mgr.snapshot_count(), 0);
    assert!(!mgr.initialized);
}

// Integration tests that require git are skipped in CI
// but can be run locally with: cargo test -- --ignored

#[test]
fn test_unquote_git_path_plain() {
    assert_eq!(unquote_git_path("src/main.rs"), "src/main.rs");
}

#[test]
fn test_unquote_git_path_quoted() {
    assert_eq!(
        unquote_git_path("\"path with spaces/file.rs\""),
        "path with spaces/file.rs"
    );
}

#[test]
fn test_unquote_git_path_escaped() {
    assert_eq!(
        unquote_git_path("\"path\\\\with\\\\backslashes\""),
        "path\\with\\backslashes"
    );
}

#[test]
fn test_diff_summary_default() {
    let summary = DiffSummary::default();
    assert_eq!(summary.additions, 0);
    assert_eq!(summary.deletions, 0);
    assert_eq!(summary.files, 0);
}

#[test]
fn test_diff_status_equality() {
    assert_eq!(DiffStatus::Added, DiffStatus::Added);
    assert_ne!(DiffStatus::Added, DiffStatus::Modified);
    assert_ne!(DiffStatus::Modified, DiffStatus::Deleted);
}

#[test]
fn test_latest_snapshot_empty() {
    let mgr = SnapshotManager::new("/tmp/test-project");
    assert!(mgr.latest_snapshot().is_none());
}

#[test]
#[ignore]
fn test_snapshot_diff_numstat() {
    let tmp = tempfile::TempDir::new().unwrap();
    let project_dir = tmp.path().to_string_lossy().to_string();

    Command::new("git")
        .args(["init"])
        .current_dir(&project_dir)
        .output()
        .unwrap();

    // Create initial file
    std::fs::write(tmp.path().join("test.txt"), "line1\nline2\nline3\n").unwrap();
    let mut mgr = SnapshotManager::new(&project_dir);
    let hash1 = mgr.track().unwrap();

    // Modify file — add 2 lines, remove 1
    std::fs::write(
        tmp.path().join("test.txt"),
        "line1\nline2_modified\nline3\nnew_line4\nnew_line5\n",
    )
    .unwrap();
    let hash2 = mgr.track().unwrap();

    let stats = mgr.diff_numstat(&hash1, &hash2);
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].file_path, "test.txt");
    assert!(stats[0].additions > 0);
    assert!(!stats[0].is_binary);
}

#[test]
#[ignore]
fn test_snapshot_diff_full() {
    let tmp = tempfile::TempDir::new().unwrap();
    let project_dir = tmp.path().to_string_lossy().to_string();

    Command::new("git")
        .args(["init"])
        .current_dir(&project_dir)
        .output()
        .unwrap();

    std::fs::write(tmp.path().join("hello.rs"), "fn main() {}\n").unwrap();
    let mut mgr = SnapshotManager::new(&project_dir);
    let hash1 = mgr.track().unwrap();

    std::fs::write(
        tmp.path().join("hello.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();
    std::fs::write(tmp.path().join("new_file.txt"), "new content\n").unwrap();
    let hash2 = mgr.track().unwrap();

    let diffs = mgr.diff_full(&hash1, &hash2);
    assert!(diffs.len() >= 2);

    let hello_diff = diffs.iter().find(|d| d.file_path == "hello.rs").unwrap();
    assert_eq!(hello_diff.status, DiffStatus::Modified);
    assert!(hello_diff.before.contains("fn main()"));
    assert!(hello_diff.after.contains("println!"));

    let new_diff = diffs
        .iter()
        .find(|d| d.file_path == "new_file.txt")
        .unwrap();
    assert_eq!(new_diff.status, DiffStatus::Added);
    assert!(new_diff.before.is_empty());
    assert!(new_diff.after.contains("new content"));
}

#[test]
#[ignore]
fn test_snapshot_diff_summary() {
    let tmp = tempfile::TempDir::new().unwrap();
    let project_dir = tmp.path().to_string_lossy().to_string();

    Command::new("git")
        .args(["init"])
        .current_dir(&project_dir)
        .output()
        .unwrap();

    std::fs::write(tmp.path().join("a.txt"), "line1\n").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "line1\n").unwrap();
    let mut mgr = SnapshotManager::new(&project_dir);
    let hash1 = mgr.track().unwrap();

    std::fs::write(tmp.path().join("a.txt"), "line1\nline2\n").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "modified\n").unwrap();
    let hash2 = mgr.track().unwrap();

    let summary = mgr.diff_summary(&hash1, &hash2);
    assert_eq!(summary.files, 2);
    assert!(summary.additions > 0);
}

#[test]
#[ignore]
fn test_snapshot_track_and_patch() {
    let tmp = tempfile::TempDir::new().unwrap();
    let project_dir = tmp.path().to_string_lossy().to_string();

    // Initialize a git repo in the project dir
    Command::new("git")
        .args(["init"])
        .current_dir(&project_dir)
        .output()
        .unwrap();

    // Create a file
    std::fs::write(tmp.path().join("test.txt"), "hello").unwrap();

    let mut mgr = SnapshotManager::new(&project_dir);
    let hash1 = mgr.track();
    assert!(hash1.is_some());
    assert_eq!(mgr.snapshot_count(), 1);

    // Modify the file
    std::fs::write(tmp.path().join("test.txt"), "hello world").unwrap();

    let changed = mgr.patch(hash1.as_ref().unwrap());
    assert!(changed.contains(&"test.txt".to_string()));
}
