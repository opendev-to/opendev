//! Cross-platform file locking for concurrent session access.
//!
//! Uses `fd-lock` for exclusive locks on session files.
//! This prevents corruption when multiple channel handlers
//! write to the same session simultaneously.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fd_lock::RwLock;
use tracing::debug;

/// Exclusive file lock guard.
///
/// Wraps `fd-lock` to provide a cross-platform file locking mechanism.
/// The lock is released when the guard is dropped.
pub struct FileLock {
    /// We keep the RwLock in locked state. On drop, the RwLock drops
    /// which releases the OS lock and closes the file.
    _rw_lock: RwLock<File>,
    lock_path: PathBuf,
}

impl FileLock {
    /// Acquire an exclusive lock on a file path.
    ///
    /// Creates a `.lock` sidecar file and acquires an exclusive lock on it.
    /// Retries with 50ms intervals up to `timeout`.
    pub fn acquire(path: &Path, timeout: Duration) -> io::Result<Self> {
        let lock_path = path.with_extension(
            path.extension()
                .map(|e| format!("{}.lock", e.to_string_lossy()))
                .unwrap_or_else(|| "lock".to_string()),
        );

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&lock_path)?;

        let mut rw_lock = RwLock::new(file);
        let start = Instant::now();

        loop {
            // Use is_ok() to check without holding the borrow across the move
            let acquired = rw_lock.try_write().is_ok();
            if acquired {
                debug!("Acquired lock on {}", path.display());
                return Ok(Self {
                    _rw_lock: rw_lock,
                    lock_path,
                });
            }

            if start.elapsed() > timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "Could not acquire lock on {} after {:?}",
                        path.display(),
                        timeout,
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Release the lock explicitly (also happens on drop).
    pub fn release(self) {
        // Just drop self, which drops the RwLock -> releases OS lock
        drop(self);
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Clean up the lock file
        if let Err(e) = std::fs::remove_file(&self.lock_path)
            && e.kind() != io::ErrorKind::NotFound
        {
            debug!(
                "Could not remove lock file {}: {}",
                self.lock_path.display(),
                e
            );
        }
    }
}

/// Convenience function: execute a closure while holding an exclusive file lock.
pub fn with_file_lock<T, F>(path: &Path, timeout: Duration, f: F) -> io::Result<T>
where
    F: FnOnce() -> T,
{
    let _lock = FileLock::acquire(path, timeout)?;
    Ok(f())
}

#[cfg(test)]
#[path = "file_locks_tests.rs"]
mod tests;
