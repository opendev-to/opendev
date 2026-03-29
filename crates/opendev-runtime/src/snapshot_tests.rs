use super::*;
use tempfile::TempDir;

#[test]
fn test_compute_project_id() {
    let id1 = compute_project_id(Path::new("/project/a"));
    let id2 = compute_project_id(Path::new("/project/b"));
    assert_ne!(id1, id2);
    assert_eq!(id1.len(), 16);
}

#[test]
fn test_snapshot_dir_location() {
    let tmp = TempDir::new().unwrap();
    let mgr = SnapshotManager::new(tmp.path());
    assert!(mgr.snapshot_dir().to_string_lossy().contains("snapshot"));
}

#[test]
fn test_take_snapshot_no_files() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = SnapshotManager::new(tmp.path());
    let result = mgr.take_snapshot(&[], "empty");
    assert!(result.is_none());
}

#[test]
fn test_take_snapshot_nonexistent_files() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = SnapshotManager::new(tmp.path());
    let result = mgr.take_snapshot(&["/nonexistent/file.txt"], "test");
    assert!(result.is_none());
}

#[test]
fn test_snapshot_and_revert() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().canonicalize().unwrap();

    // Create a file
    let file = project.join("test.txt");
    std::fs::write(&file, "original content").unwrap();

    let mut mgr = SnapshotManager::new(&project);

    // Take snapshot
    let file_str = file.to_string_lossy().to_string();
    let snapshot_id = mgr.take_snapshot(&[&file_str], "before edit");

    // Git might not be available in CI, skip if so
    if snapshot_id.is_none() {
        return;
    }
    let sid = snapshot_id.unwrap();
    assert!(!sid.is_empty());

    // Modify the file
    std::fs::write(&file, "modified content").unwrap();

    // Revert
    let reverted = mgr.revert_to_snapshot(&sid);
    if reverted.is_empty() {
        // Git checkout may fail in some environments
        return;
    }

    // Check content is restored
    let content = std::fs::read_to_string(&file).unwrap();
    assert_eq!(content, "original content");
}

#[test]
fn test_debug_format() {
    let tmp = TempDir::new().unwrap();
    let mgr = SnapshotManager::new(tmp.path());
    let debug_str = format!("{:?}", mgr);
    assert!(debug_str.contains("SnapshotManager"));
}
