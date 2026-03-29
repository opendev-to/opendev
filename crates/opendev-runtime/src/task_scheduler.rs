//! Background task scheduler for deferred and periodic work (#95).
//!
//! Provides [`TaskScheduler`] which manages one-shot and recurring async tasks
//! using `tokio::spawn` and `tokio::time`.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::debug;

/// Unique identifier for a scheduled task.
pub type TaskId = u64;

/// Internal state shared across clones.
struct SchedulerInner {
    next_id: AtomicU64,
    tasks: Mutex<HashMap<TaskId, TaskEntry>>,
}

struct TaskEntry {
    label: String,
    handle: JoinHandle<()>,
}

/// A scheduler for one-shot and periodic background tasks.
///
/// All tasks are cancelled when [`TaskScheduler::shutdown`] is called or when
/// the scheduler is dropped.
#[derive(Clone)]
pub struct TaskScheduler {
    inner: Arc<SchedulerInner>,
}

impl TaskScheduler {
    /// Create a new task scheduler.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(SchedulerInner {
                next_id: AtomicU64::new(1),
                tasks: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Schedule a one-shot task that executes after `delay`.
    ///
    /// Returns a [`TaskId`] that can be used to cancel the task.
    pub fn schedule_once<F, Fut>(&self, delay: Duration, label: impl Into<String>, f: F) -> TaskId
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let label_str = label.into();
        let inner = Arc::clone(&self.inner);
        let task_label = label_str.clone();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            debug!("Running one-shot task {id} ({task_label})");
            f().await;
            // Remove self from the map after completion.
            inner.tasks.lock().await.remove(&id);
        });

        {
            let inner = Arc::clone(&self.inner);
            let label_str = label_str.clone();
            tokio::spawn(async move {
                inner.tasks.lock().await.insert(
                    id,
                    TaskEntry {
                        label: label_str,
                        handle,
                    },
                );
            });
        }

        id
    }

    /// Schedule a periodic task that runs every `interval`.
    ///
    /// The task function receives the current tick count (starting at 1).
    /// The first execution happens after `interval` elapses.
    ///
    /// Returns a [`TaskId`] that can be used to cancel the task.
    pub fn schedule_periodic<F, Fut>(
        &self,
        interval: Duration,
        label: impl Into<String>,
        f: F,
    ) -> TaskId
    where
        F: Fn(u64) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let label_str = label.into();
        let task_label = label_str.clone();

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // First tick fires immediately — skip it so the first execution
            // happens after one full interval.
            ticker.tick().await;

            let mut tick_count: u64 = 0;
            loop {
                ticker.tick().await;
                tick_count += 1;
                debug!("Periodic task {id} ({task_label}) tick {tick_count}");
                f(tick_count).await;
            }
        });

        let inner = Arc::clone(&self.inner);
        let label_owned = label_str;
        tokio::spawn(async move {
            inner.tasks.lock().await.insert(
                id,
                TaskEntry {
                    label: label_owned,
                    handle,
                },
            );
        });

        id
    }

    /// Cancel a previously scheduled task.
    ///
    /// Returns `true` if the task was found and cancelled.
    pub async fn cancel(&self, id: TaskId) -> bool {
        if let Some(entry) = self.inner.tasks.lock().await.remove(&id) {
            entry.handle.abort();
            debug!("Cancelled task {id} ({})", entry.label);
            true
        } else {
            false
        }
    }

    /// Return the number of active (not yet completed / cancelled) tasks.
    pub async fn active_count(&self) -> usize {
        self.inner.tasks.lock().await.len()
    }

    /// Cancel all tasks and shut down the scheduler.
    pub async fn shutdown(&self) {
        let mut tasks = self.inner.tasks.lock().await;
        for (id, entry) in tasks.drain() {
            entry.handle.abort();
            debug!("Shutdown: cancelled task {id} ({})", entry.label);
        }
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TaskScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskScheduler").finish()
    }
}

/// Convenience: schedule a one-shot closure that returns a boxed future.
pub fn boxed_task<F>(f: F) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>
where
    F: Future<Output = ()> + Send + 'static,
{
    Box::pin(f)
}

#[cfg(test)]
#[path = "task_scheduler_tests.rs"]
mod tests;
