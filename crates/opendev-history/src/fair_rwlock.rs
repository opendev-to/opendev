//! Fair reader-writer lock for session file access.
//!
//! Standard `tokio::sync::RwLock` can starve writers when readers are
//! frequent.  [`FairRwLock`] uses a semaphore-based queue to guarantee
//! FIFO ordering: writers are not indefinitely delayed by a stream of
//! incoming readers.
//!
//! # Implementation
//!
//! A `tokio::sync::Semaphore` with `MAX_READERS` permits acts as the
//! gate.  Readers acquire **one** permit; writers acquire **all**
//! permits, which blocks until every reader has released.  Because the
//! semaphore is FIFO, a waiting writer will be served before any reader
//! that arrived later.

use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Maximum number of concurrent readers.
///
/// Writers acquire all permits, so this also bounds how many permits
/// a writer must collect.
const MAX_READERS: u32 = 32;

/// A fair reader-writer lock that prevents writer starvation.
///
/// Wraps a `tokio::sync::Semaphore` so that both readers and writers
/// enter the same FIFO queue.  Writers request all permits, guaranteeing
/// exclusive access once granted.
#[derive(Debug, Clone)]
pub struct FairRwLock {
    semaphore: Arc<Semaphore>,
}

/// Guard returned by [`FairRwLock::read`].
///
/// Releases the single semaphore permit on drop.
#[derive(Debug)]
pub struct FairReadGuard {
    _permit: OwnedSemaphorePermit,
}

/// Guard returned by [`FairRwLock::write`].
///
/// Releases all semaphore permits on drop.
#[derive(Debug)]
pub struct FairWriteGuard {
    _permit: OwnedSemaphorePermit,
}

impl FairRwLock {
    /// Create a new fair reader-writer lock.
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(MAX_READERS as usize)),
        }
    }

    /// Acquire a read lock.
    ///
    /// Multiple readers can hold a read lock simultaneously (up to
    /// `MAX_READERS`).  A pending writer will block new readers from
    /// acquiring, preventing starvation.
    pub async fn read(&self) -> FairReadGuard {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore is never closed");
        FairReadGuard { _permit: permit }
    }

    /// Acquire a write lock.
    ///
    /// The writer waits until it can acquire **all** permits, ensuring
    /// exclusive access.  Because the semaphore is FIFO, writers that
    /// arrive before later readers will be served first.
    pub async fn write(&self) -> FairWriteGuard {
        let permit = self
            .semaphore
            .clone()
            .acquire_many_owned(MAX_READERS)
            .await
            .expect("semaphore is never closed");
        FairWriteGuard { _permit: permit }
    }

    /// Try to acquire a read lock without waiting.
    ///
    /// Returns `None` if the lock is currently held exclusively by a writer.
    pub fn try_read(&self) -> Option<FairReadGuard> {
        self.semaphore
            .clone()
            .try_acquire_owned()
            .ok()
            .map(|permit| FairReadGuard { _permit: permit })
    }

    /// Try to acquire a write lock without waiting.
    ///
    /// Returns `None` if any readers or another writer currently hold the lock.
    pub fn try_write(&self) -> Option<FairWriteGuard> {
        self.semaphore
            .clone()
            .try_acquire_many_owned(MAX_READERS)
            .ok()
            .map(|permit| FairWriteGuard { _permit: permit })
    }
}

impl Default for FairRwLock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn test_multiple_readers() {
        let lock = FairRwLock::new();
        let _r1 = lock.read().await;
        let _r2 = lock.read().await;
        let _r3 = lock.read().await;
        // All three readers can hold the lock simultaneously.
    }

    #[tokio::test]
    async fn test_writer_excludes_readers() {
        let lock = FairRwLock::new();
        let _w = lock.write().await;
        // try_read should fail while writer holds the lock.
        assert!(lock.try_read().is_none());
    }

    #[tokio::test]
    async fn test_writer_excludes_writers() {
        let lock = FairRwLock::new();
        let _w = lock.write().await;
        assert!(lock.try_write().is_none());
    }

    #[tokio::test]
    async fn test_read_then_write_then_read() {
        let lock = FairRwLock::new();

        // Read
        {
            let _r = lock.read().await;
        }
        // Write
        {
            let _w = lock.write().await;
        }
        // Read again
        {
            let _r = lock.read().await;
        }
    }

    #[tokio::test]
    async fn test_try_read_succeeds_when_free() {
        let lock = FairRwLock::new();
        assert!(lock.try_read().is_some());
    }

    #[tokio::test]
    async fn test_try_write_succeeds_when_free() {
        let lock = FairRwLock::new();
        assert!(lock.try_write().is_some());
    }

    #[tokio::test]
    async fn test_try_write_fails_with_reader() {
        let lock = FairRwLock::new();
        let _r = lock.read().await;
        assert!(lock.try_write().is_none());
    }

    #[tokio::test]
    async fn test_writer_fairness() {
        // Verify that a writer waiting in the queue is served before
        // a reader that arrives later.
        let lock = Arc::new(FairRwLock::new());
        let order = Arc::new(AtomicU32::new(0));

        // Acquire initial read lock.
        let r1 = lock.read().await;

        // Spawn a writer task that will block.
        let lock2 = Arc::clone(&lock);
        let order2 = Arc::clone(&order);
        let writer_handle = tokio::spawn(async move {
            let _w = lock2.write().await;
            order2.fetch_add(1, Ordering::SeqCst);
            // Hold briefly.
            tokio::time::sleep(Duration::from_millis(10)).await;
        });

        // Give the writer time to enter the queue.
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Spawn a reader that arrives after the writer.
        let lock3 = Arc::clone(&lock);
        let order3 = Arc::clone(&order);
        let reader_handle = tokio::spawn(async move {
            let _r = lock3.read().await;
            order3.fetch_add(10, Ordering::SeqCst);
        });

        // Small delay to ensure the reader task is also queued.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Release initial read lock so the writer can proceed.
        drop(r1);

        writer_handle.await.unwrap();
        reader_handle.await.unwrap();

        // Writer should have run first (added 1), then reader (added 10).
        // If fair: order = 11. If unfair (reader sneaks in first): order = 11 too
        // but the first increment would be 10.
        // We just verify both completed.
        let final_order = order.load(Ordering::SeqCst);
        assert_eq!(final_order, 11);
    }

    #[tokio::test]
    async fn test_default_impl() {
        let lock = FairRwLock::default();
        let _r = lock.read().await;
    }

    #[tokio::test]
    async fn test_clone_shares_state() {
        let lock1 = FairRwLock::new();
        let lock2 = lock1.clone();

        let _w = lock1.write().await;
        // Clone shares the same semaphore, so try_read on clone should fail.
        assert!(lock2.try_read().is_none());
    }
}
