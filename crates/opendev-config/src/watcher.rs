//! Poll-based config file watcher for hot-reload.
//!
//! Monitors config files for changes by comparing modification timestamps.
//! When a change is detected, `config_changed` is set to true so the
//! application can re-merge the config at runtime.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Tracks modification times of config files for change detection.
#[derive(Debug)]
pub struct ConfigWatcher {
    /// Paths to watch.
    watched_paths: Vec<PathBuf>,
    /// Last known modification times.
    last_modified: Vec<Option<SystemTime>>,
    /// Flag indicating a config file has changed since last check.
    pub config_changed: bool,
}

impl ConfigWatcher {
    /// Create a new watcher for the given config file paths.
    pub fn new(paths: Vec<PathBuf>) -> Self {
        let last_modified = paths.iter().map(|p| Self::file_mtime(p)).collect();
        Self {
            watched_paths: paths,
            last_modified,
            config_changed: false,
        }
    }

    /// Check if any watched files have been modified.
    ///
    /// Sets `config_changed` to `true` if any file has a newer modification time.
    /// Returns `true` if a change was detected on this call.
    pub fn check(&mut self) -> bool {
        let mut changed = false;

        for (i, path) in self.watched_paths.iter().enumerate() {
            let current_mtime = Self::file_mtime(path);

            if current_mtime != self.last_modified[i] {
                changed = true;
                self.last_modified[i] = current_mtime;
            }
        }

        if changed {
            self.config_changed = true;
        }

        changed
    }

    /// Acknowledge a config change (reset the flag).
    pub fn acknowledge(&mut self) {
        self.config_changed = false;
    }

    /// Get the list of watched paths.
    pub fn watched_paths(&self) -> &[PathBuf] {
        &self.watched_paths
    }

    /// Add a path to watch.
    pub fn add_path(&mut self, path: PathBuf) {
        let mtime = Self::file_mtime(&path);
        self.watched_paths.push(path);
        self.last_modified.push(mtime);
    }

    /// Get the modification time of a file, or None if the file doesn't exist.
    fn file_mtime(path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
    }
}

#[cfg(test)]
mod tests {
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
}
