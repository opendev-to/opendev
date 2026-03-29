//! File watcher using the `notify` crate for native filesystem event detection.
//!
//! Monitors a working directory for file changes using OS-level filesystem
//! notifications (FSEvents on macOS, inotify on Linux, ReadDirectoryChanges on
//! Windows). Events are debounced with a configurable interval (default 500ms).
//! Includes an inactivity timeout that stops watching after a configurable
//! period of no detected changes (default 5 minutes).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Default debounce interval for filesystem events.
const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(500);

/// Default inactivity timeout before the watcher shuts down.
const DEFAULT_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Directories to ignore by default.
const DEFAULT_IGNORE_DIRS: &[&str] = &[".git", "target", "node_modules", ".opendev", ".DS_Store"];

/// A file change detected by the watcher.
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Path to the changed file.
    pub path: PathBuf,
    /// The kind of change detected.
    pub kind: FileChangeKind,
}

/// The type of file change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeKind {
    /// A file was created (new file appeared).
    Created,
    /// A file was modified (mtime changed).
    Modified,
    /// A file was deleted (previously tracked file is gone).
    Deleted,
}

/// Configuration for the [`FileWatcher`].
#[derive(Debug, Clone)]
pub struct FileWatcherConfig {
    /// How long to debounce filesystem events before emitting.
    pub debounce: Duration,
    /// How long without changes before the watcher stops.
    pub inactivity_timeout: Duration,
    /// Directory names to ignore (e.g., ".git", "target").
    pub ignore_patterns: Vec<String>,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            debounce: DEFAULT_DEBOUNCE,
            inactivity_timeout: DEFAULT_INACTIVITY_TIMEOUT,
            ignore_patterns: DEFAULT_IGNORE_DIRS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        }
    }
}

/// Monitors a working directory for file changes using OS-native filesystem
/// notifications via the `notify` crate.
///
/// The watcher runs as an async task and sends [`FileChange`] events through
/// a channel. It automatically stops after a configurable inactivity timeout.
pub struct FileWatcher {
    /// Root directory to watch.
    root: PathBuf,
    /// Configuration.
    config: FileWatcherConfig,
    /// Cancel token to stop the watcher externally.
    cancel: tokio::sync::watch::Sender<bool>,
}

impl FileWatcher {
    /// Create a new file watcher for the given directory.
    pub fn new(root: impl Into<PathBuf>, config: FileWatcherConfig) -> Self {
        let (cancel, _) = tokio::sync::watch::channel(false);
        Self {
            root: root.into(),
            config,
            cancel,
        }
    }

    /// Create a watcher with default configuration.
    pub fn with_defaults(root: impl Into<PathBuf>) -> Self {
        Self::new(root, FileWatcherConfig::default())
    }

    /// Start watching and return a receiver for file changes.
    ///
    /// The watcher runs in a background tokio task. It will stop when:
    /// - The inactivity timeout is reached (no changes detected)
    /// - [`stop`] is called
    /// - The `FileWatcher` is dropped
    pub fn start(&self) -> mpsc::UnboundedReceiver<FileChange> {
        let (tx, rx) = mpsc::unbounded_channel();
        let root = self.root.clone();
        let config = self.config.clone();
        let mut cancel_rx = self.cancel.subscribe();

        tokio::spawn(async move {
            // Create a std mpsc channel for the notify debouncer callback.
            let (notify_tx, notify_rx) = std::sync::mpsc::channel();

            let debouncer = new_debouncer(config.debounce, move |result| {
                let _ = notify_tx.send(result);
            });

            let mut debouncer = match debouncer {
                Ok(d) => d,
                Err(e) => {
                    warn!(error = %e, "Failed to create file watcher");
                    return;
                }
            };

            if let Err(e) = debouncer.watcher().watch(&root, RecursiveMode::Recursive) {
                warn!(
                    root = %root.display(),
                    error = %e,
                    "Failed to start watching directory"
                );
                return;
            }

            let ignore_patterns = Arc::new(config.ignore_patterns);

            info!(
                root = %root.display(),
                "FileWatcher started (notify)"
            );

            let mut last_change = tokio::time::Instant::now();
            let mut check_interval = tokio::time::interval(Duration::from_millis(100));

            loop {
                tokio::select! {
                    _ = check_interval.tick() => {
                        // Check inactivity timeout
                        if last_change.elapsed() >= config.inactivity_timeout {
                            info!(
                                timeout_secs = config.inactivity_timeout.as_secs(),
                                "FileWatcher stopped: inactivity timeout"
                            );
                            break;
                        }

                        // Drain all available events from the notify channel
                        while let Ok(result) = notify_rx.try_recv() {
                            match result {
                                Ok(events) => {
                                    for event in events {
                                        let path = &event.path;

                                        // Skip paths containing ignored directory names
                                        if should_ignore(path, &ignore_patterns) {
                                            continue;
                                        }

                                        let kind = if path.exists() {
                                            FileChangeKind::Modified
                                        } else {
                                            FileChangeKind::Deleted
                                        };

                                        last_change = tokio::time::Instant::now();
                                        debug!(
                                            path = %path.display(),
                                            kind = ?kind,
                                            "File change detected"
                                        );

                                        if tx.send(FileChange {
                                            path: path.clone(),
                                            kind,
                                        }).is_err() {
                                            debug!("FileWatcher channel closed, stopping");
                                            return;
                                        }
                                    }
                                }
                                Err(errors) => {
                                    warn!(errors = ?errors, "File watcher errors");
                                }
                            }
                        }
                    }
                    result = cancel_rx.changed() => {
                        if result.is_err() || *cancel_rx.borrow() {
                            info!("FileWatcher stopped: cancelled");
                            break;
                        }
                    }
                }
            }

            // debouncer is dropped here, which stops the native watcher
        });

        rx
    }

    /// Stop the watcher.
    pub fn stop(&self) {
        let _ = self.cancel.send(true);
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Check whether a path should be ignored based on the ignore patterns.
fn should_ignore(path: &Path, ignore_patterns: &[String]) -> bool {
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if ignore_patterns.iter().any(|p| name.as_ref() == p.as_str()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[path = "file_watcher_tests.rs"]
mod tests;
