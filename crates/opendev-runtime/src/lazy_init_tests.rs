use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

#[tokio::test]
async fn test_lazy_init_on_first_use() {
    let lazy: LazySubsystem<String> = LazySubsystem::new("test");
    assert!(!lazy.is_initialized());
    assert!(lazy.try_get().is_none());

    let value = lazy.get(|| async { "hello".to_string() }).await;
    assert_eq!(value, "hello");
    assert!(lazy.is_initialized());
}

#[tokio::test]
async fn test_lazy_init_only_once() {
    let counter = Arc::new(AtomicU32::new(0));
    let lazy: LazySubsystem<u32> = LazySubsystem::new("counter");

    let c1 = Arc::clone(&counter);
    let v1 = lazy
        .get(|| {
            let c = Arc::clone(&c1);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                42
            }
        })
        .await;

    let c2 = Arc::clone(&counter);
    let v2 = lazy
        .get(|| {
            let c = Arc::clone(&c2);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                99
            }
        })
        .await;

    assert_eq!(*v1, 42);
    assert_eq!(*v2, 42); // Same value, init not called again.
    assert_eq!(counter.load(Ordering::SeqCst), 1); // Init ran only once.
}

#[tokio::test]
async fn test_lazy_fallible_init_success() {
    let lazy: LazySubsystem<String> = LazySubsystem::new("fallible");

    let result: Result<&String, &str> = lazy
        .get_or_try_init(|| async { Ok("success".to_string()) })
        .await;

    assert_eq!(result.unwrap(), "success");
    assert!(lazy.is_initialized());
}

#[tokio::test]
async fn test_lazy_fallible_init_failure_allows_retry() {
    let lazy: LazySubsystem<String> = LazySubsystem::new("retry");
    let counter = Arc::new(AtomicU32::new(0));

    // First attempt fails.
    let c1 = Arc::clone(&counter);
    let result: Result<&String, String> = lazy
        .get_or_try_init(|| {
            let c = Arc::clone(&c1);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err("fail".to_string())
            }
        })
        .await;
    assert!(result.is_err());
    assert!(!lazy.is_initialized());

    // Second attempt succeeds.
    let c2 = Arc::clone(&counter);
    let result: Result<&String, String> = lazy
        .get_or_try_init(|| {
            let c = Arc::clone(&c2);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok("ok".to_string())
            }
        })
        .await;
    assert_eq!(result.unwrap(), "ok");
    assert!(lazy.is_initialized());
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_lazy_clone_shares_state() {
    let lazy1: LazySubsystem<u32> = LazySubsystem::new("shared");
    let lazy2 = lazy1.clone();

    let _ = lazy1.get(|| async { 100 }).await;
    assert!(lazy2.is_initialized());
    assert_eq!(*lazy2.try_get().unwrap(), 100);
}

#[tokio::test]
async fn test_lazy_name() {
    let lazy: LazySubsystem<()> = LazySubsystem::new("MySubsystem");
    assert_eq!(lazy.name(), "MySubsystem");
}

#[tokio::test]
async fn test_create_lazy_subsystems() {
    let (lsp, mcp, emb) = create_lazy_subsystems::<String, String, String>();
    assert_eq!(lsp.name(), "LSP");
    assert_eq!(mcp.name(), "MCP");
    assert_eq!(emb.name(), "Embeddings");
    assert!(!lsp.is_initialized());
    assert!(!mcp.is_initialized());
    assert!(!emb.is_initialized());
}

#[test]
fn test_sync_lazy_init() {
    let lazy = SyncLazy::new("sync-test");
    assert!(!lazy.is_initialized());

    let val = lazy.get_or_init(|| 42u32);
    assert_eq!(*val, 42);
    assert!(lazy.is_initialized());

    // Second call returns same value.
    let val2 = lazy.get_or_init(|| 99);
    assert_eq!(*val2, 42);
}

#[test]
fn test_sync_lazy_try_get() {
    let lazy = SyncLazy::new("sync-try");
    assert!(lazy.try_get().is_none());

    lazy.get_or_init(|| "hello");
    assert_eq!(*lazy.try_get().unwrap(), "hello");
}

#[test]
fn test_sync_lazy_name() {
    let lazy = SyncLazy::<()>::new("test-name");
    assert_eq!(lazy.name(), "test-name");
}

#[tokio::test]
async fn test_lazy_debug_format() {
    let lazy: LazySubsystem<u32> = LazySubsystem::new("dbg");
    let debug_str = format!("{:?}", lazy);
    assert!(debug_str.contains("LazySubsystem"));
    assert!(debug_str.contains("dbg"));
    assert!(debug_str.contains("false"));

    let _ = lazy.get(|| async { 1 }).await;
    let debug_str = format!("{:?}", lazy);
    assert!(debug_str.contains("true"));
}

#[test]
fn test_sync_lazy_debug_format() {
    let lazy = SyncLazy::<u32>::new("sync-dbg");
    let debug_str = format!("{:?}", lazy);
    assert!(debug_str.contains("SyncLazy"));
    assert!(debug_str.contains("sync-dbg"));
}
