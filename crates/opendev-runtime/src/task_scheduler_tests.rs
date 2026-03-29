use super::*;
use std::sync::atomic::AtomicBool;
use tokio::time;

#[tokio::test]
async fn test_schedule_once_executes() {
    let scheduler = TaskScheduler::new();
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&flag);

    scheduler.schedule_once(Duration::from_millis(10), "test", move || {
        let f = Arc::clone(&flag_clone);
        async move {
            f.store(true, Ordering::SeqCst);
        }
    });

    time::sleep(Duration::from_millis(50)).await;
    assert!(flag.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_schedule_once_respects_delay() {
    let scheduler = TaskScheduler::new();
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&flag);

    scheduler.schedule_once(Duration::from_millis(200), "delayed", move || {
        let f = Arc::clone(&flag_clone);
        async move {
            f.store(true, Ordering::SeqCst);
        }
    });

    // Should not have fired yet after 10ms.
    time::sleep(Duration::from_millis(10)).await;
    assert!(!flag.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_schedule_periodic_fires_multiple_times() {
    let scheduler = TaskScheduler::new();
    let counter = Arc::new(AtomicU64::new(0));
    let counter_clone = Arc::clone(&counter);

    scheduler.schedule_periodic(Duration::from_millis(15), "ticker", move |_tick| {
        let c = Arc::clone(&counter_clone);
        async move {
            c.fetch_add(1, Ordering::SeqCst);
        }
    });

    time::sleep(Duration::from_millis(100)).await;
    let count = counter.load(Ordering::SeqCst);
    // Should have fired multiple times (at least 3 in 100ms with 15ms interval).
    assert!(count >= 3, "expected >= 3 ticks, got {count}");
}

#[tokio::test]
async fn test_cancel_task() {
    let scheduler = TaskScheduler::new();
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&flag);

    let id = scheduler.schedule_once(Duration::from_millis(100), "cancel_me", move || {
        let f = Arc::clone(&flag_clone);
        async move {
            f.store(true, Ordering::SeqCst);
        }
    });

    // Give the spawn a moment to register.
    time::sleep(Duration::from_millis(5)).await;

    let cancelled = scheduler.cancel(id).await;
    assert!(cancelled);

    time::sleep(Duration::from_millis(150)).await;
    assert!(!flag.load(Ordering::SeqCst), "task should not have fired");
}

#[tokio::test]
async fn test_shutdown_cancels_all() {
    let scheduler = TaskScheduler::new();
    let counter = Arc::new(AtomicU64::new(0));

    for i in 0..5 {
        let c = Arc::clone(&counter);
        scheduler.schedule_once(Duration::from_millis(200), format!("task-{i}"), move || {
            let c = Arc::clone(&c);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
            }
        });
    }

    time::sleep(Duration::from_millis(10)).await;
    scheduler.shutdown().await;

    time::sleep(Duration::from_millis(300)).await;
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn test_cancel_nonexistent_returns_false() {
    let scheduler = TaskScheduler::new();
    assert!(!scheduler.cancel(999).await);
}

#[test]
fn test_default_scheduler() {
    let _scheduler = TaskScheduler::default();
}

#[test]
fn test_debug_format() {
    let scheduler = TaskScheduler::new();
    let debug_str = format!("{:?}", scheduler);
    assert!(debug_str.contains("TaskScheduler"));
}
