use super::*;
use crate::protocol::{Position, SourceRange, SymbolKind};

fn make_symbol(name: &str) -> UnifiedSymbolInfo {
    UnifiedSymbolInfo {
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from("/test.rs"),
        range: SourceRange::new(Position::new(0, 0), Position::new(10, 0)),
        selection_range: None,
        container_name: None,
        detail: None,
    }
}

#[tokio::test]
async fn test_cache_put_and_get() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut cache = SymbolCache::new(Some(tmp.path().to_path_buf()), None);

    let ws = PathBuf::from("/workspace");
    let symbols = vec![make_symbol("foo"), make_symbol("bar")];

    cache.put(&ws, "test_query", symbols.clone()).await;

    let result = cache.get(&ws, "test_query").await;
    assert!(result.is_some());
    let cached = result.unwrap();
    assert_eq!(cached.len(), 2);
    assert_eq!(cached[0].name, "foo");
    assert_eq!(cached[1].name, "bar");
}

#[tokio::test]
async fn test_cache_miss() {
    let mut cache = SymbolCache::new(None, None);
    let ws = PathBuf::from("/workspace");
    assert!(cache.get(&ws, "missing").await.is_none());
}

#[tokio::test]
async fn test_cache_expiration() {
    let mut cache = SymbolCache::new(None, Some(0)); // 0 second TTL
    let ws = PathBuf::from("/workspace");
    cache.put(&ws, "q", vec![make_symbol("x")]).await;
    // Should be expired immediately
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert!(cache.get(&ws, "q").await.is_none());
}

#[tokio::test]
async fn test_invalidate_workspace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut cache = SymbolCache::new(Some(tmp.path().to_path_buf()), None);

    let ws1 = PathBuf::from("/workspace1");
    let ws2 = PathBuf::from("/workspace2");

    cache.put(&ws1, "q1", vec![make_symbol("a")]).await;
    cache.put(&ws2, "q2", vec![make_symbol("b")]).await;

    cache.invalidate_workspace(&ws1).await;

    assert!(cache.get(&ws1, "q1").await.is_none());
    assert!(cache.get(&ws2, "q2").await.is_some());
}

#[tokio::test]
async fn test_cache_clear() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut cache = SymbolCache::new(Some(tmp.path().to_path_buf()), None);

    let ws = PathBuf::from("/workspace");
    cache.put(&ws, "q", vec![make_symbol("a")]).await;
    cache.clear().await;
    assert!(cache.get(&ws, "q").await.is_none());
}

#[tokio::test]
async fn test_disk_persistence() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ws = PathBuf::from("/workspace");

    // Write with one cache instance
    {
        let mut cache = SymbolCache::new(Some(tmp.path().to_path_buf()), None);
        cache.put(&ws, "q", vec![make_symbol("persisted")]).await;
    }

    // Read with a new instance
    {
        let mut cache = SymbolCache::new(Some(tmp.path().to_path_buf()), None);
        let result = cache.get(&ws, "q").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0].name, "persisted");
    }
}
