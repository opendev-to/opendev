use super::*;
use std::io::Write;

#[test]
fn test_watcher_no_change() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.json");
    std::fs::write(&path, "{}").unwrap();

    let mut watcher = ConfigWatcher::new(vec![path]);
    assert!(!watcher.config_changed);
    assert!(!watcher.check());
    assert!(!watcher.config_changed);
}

#[test]
fn test_watcher_detects_change() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.json");
    std::fs::write(&path, r#"{"v": 1}"#).unwrap();

    let mut watcher = ConfigWatcher::new(vec![path.clone()]);
    assert!(!watcher.check());

    // Modify the file — need a small delay for filesystem timestamp granularity
    std::thread::sleep(std::time::Duration::from_millis(50));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    f.write_all(b"{\"v\": 2}").unwrap();
    f.sync_all().unwrap();
    drop(f);

    assert!(watcher.check());
    assert!(watcher.config_changed);
}

#[test]
fn test_watcher_acknowledge() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.json");
    std::fs::write(&path, "{}").unwrap();

    let mut watcher = ConfigWatcher::new(vec![path]);
    watcher.config_changed = true;
    watcher.acknowledge();
    assert!(!watcher.config_changed);
}

#[test]
fn test_watcher_nonexistent_file() {
    let path = std::env::temp_dir().join("nonexistent-opendev-test-42.json");
    // Ensure it doesn't exist from a prior run
    let _ = std::fs::remove_file(&path);

    let mut watcher = ConfigWatcher::new(vec![path.clone()]);
    assert!(!watcher.check());

    // Create the file — should detect as a change
    std::fs::write(&path, "{}").unwrap();
    assert!(watcher.check());
    assert!(watcher.config_changed);

    // Cleanup
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_watcher_add_path() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path1 = tmp.path().join("a.json");
    let path2 = tmp.path().join("b.json");
    std::fs::write(&path1, "{}").unwrap();
    std::fs::write(&path2, "{}").unwrap();

    let mut watcher = ConfigWatcher::new(vec![path1]);
    assert_eq!(watcher.watched_paths().len(), 1);

    watcher.add_path(path2);
    assert_eq!(watcher.watched_paths().len(), 2);
}

#[test]
fn test_watcher_file_deleted() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.json");
    std::fs::write(&path, "{}").unwrap();

    let mut watcher = ConfigWatcher::new(vec![path.clone()]);
    assert!(!watcher.check());

    // Delete the file
    std::fs::remove_file(&path).unwrap();
    assert!(watcher.check());
    assert!(watcher.config_changed);
}
