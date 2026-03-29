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
#[path = "watcher_tests.rs"]
mod tests;
