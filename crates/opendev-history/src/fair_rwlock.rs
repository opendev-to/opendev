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
#[path = "fair_rwlock_tests.rs"]
mod tests;
