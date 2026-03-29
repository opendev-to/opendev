use super::*;

#[test]
fn test_embedding_metadata_create() {
    let embedding = vec![0.1, 0.2, 0.3];
    let meta = EmbeddingMetadata::create("hello", "test-model", embedding.clone());
    assert_eq!(meta.text, "hello");
    assert_eq!(meta.model, "test-model");
    assert!(!meta.hash.is_empty());
    assert_eq!(meta.embedding, embedding);
}

#[test]
fn test_embedding_cache_set_get() {
    let mut cache = EmbeddingCache::new("test-model");
    let embedding = vec![0.1, 0.2, 0.3];

    cache.set("hello", embedding.clone(), None);
    assert_eq!(cache.size(), 1);

    let result = cache.get("hello", None);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), &embedding);

    // Different text returns None
    assert!(cache.get("world", None).is_none());
}

#[test]
fn test_embedding_cache_model_scoping() {
    let mut cache = EmbeddingCache::new("model-a");
    cache.set("hello", vec![1.0], None);

    // Same text, same model -> found
    assert!(cache.peek("hello", Some("model-a")).is_some());

    // Same text, different model -> not found
    assert!(cache.peek("hello", Some("model-b")).is_none());
}

#[test]
fn test_embedding_cache_clear() {
    let mut cache = EmbeddingCache::new("test");
    cache.set("a", vec![1.0], None);
    cache.set("b", vec![2.0], None);
    assert_eq!(cache.size(), 2);

    cache.clear();
    assert_eq!(cache.size(), 0);
}

#[test]
fn test_embedding_cache_serialization() {
    let mut cache = EmbeddingCache::new("test-model");
    cache.set("hello", vec![0.1, 0.2], None);
    cache.set("world", vec![0.3, 0.4], None);

    let dict = cache.to_dict();
    let mut restored = EmbeddingCache::from_dict(&dict);

    assert_eq!(restored.model, "test-model");
    assert_eq!(restored.size(), 2);
    assert!(restored.get("hello", None).is_some());
    assert!(restored.get("world", None).is_some());
}

#[test]
fn test_embedding_cache_file_persistence() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("embeddings.json");

    let mut cache = EmbeddingCache::new("test-model");
    cache.set("hello", vec![0.1, 0.2, 0.3], None);
    cache.save_to_file(&path).unwrap();

    let mut loaded = EmbeddingCache::load_from_file(&path).unwrap();
    assert_eq!(loaded.size(), 1);
    assert!(loaded.get("hello", None).is_some());
}

#[test]
fn test_embedding_cache_load_missing_file() {
    let result = EmbeddingCache::load_from_file(Path::new("/nonexistent/path"));
    assert!(result.is_none());
}

// ------------------------------------------------------------------ //
// LRU eviction tests
// ------------------------------------------------------------------ //

#[test]
fn test_lru_eviction_at_capacity() {
    let mut cache = EmbeddingCache::with_max_entries("test", 3);
    cache.set("a", vec![1.0], None);
    cache.set("b", vec![2.0], None);
    cache.set("c", vec![3.0], None);
    assert_eq!(cache.size(), 3);

    // Adding a 4th entry should evict "a" (least recently used)
    cache.set("d", vec![4.0], None);
    assert_eq!(cache.size(), 3);
    assert!(cache.peek("a", None).is_none(), "a should be evicted");
    assert!(cache.peek("b", None).is_some());
    assert!(cache.peek("c", None).is_some());
    assert!(cache.peek("d", None).is_some());
}

#[test]
fn test_lru_access_refreshes_order() {
    let mut cache = EmbeddingCache::with_max_entries("test", 3);
    cache.set("a", vec![1.0], None);
    cache.set("b", vec![2.0], None);
    cache.set("c", vec![3.0], None);

    // Access "a" to make it most-recently-used
    cache.get("a", None);

    // Now "b" is LRU; adding "d" should evict "b"
    cache.set("d", vec![4.0], None);
    assert_eq!(cache.size(), 3);
    assert!(cache.peek("a", None).is_some(), "a was recently accessed");
    assert!(cache.peek("b", None).is_none(), "b should be evicted");
    assert!(cache.peek("c", None).is_some());
    assert!(cache.peek("d", None).is_some());
}

#[test]
fn test_lru_update_existing_key_no_eviction() {
    let mut cache = EmbeddingCache::with_max_entries("test", 3);
    cache.set("a", vec![1.0], None);
    cache.set("b", vec![2.0], None);
    cache.set("c", vec![3.0], None);

    // Updating existing key should not trigger eviction
    cache.set("a", vec![10.0], None);
    assert_eq!(cache.size(), 3);
    assert_eq!(cache.get("a", None).unwrap(), &vec![10.0]);
}

#[test]
fn test_lru_default_max_entries() {
    let cache = EmbeddingCache::new("test");
    assert_eq!(cache.max_entries, DEFAULT_MAX_ENTRIES);
}

#[test]
fn test_lru_with_max_entries_constructor() {
    let cache = EmbeddingCache::with_max_entries("test", 500);
    assert_eq!(cache.max_entries, 500);
}

#[test]
fn test_lru_clear_resets_access_order() {
    let mut cache = EmbeddingCache::with_max_entries("test", 3);
    cache.set("a", vec![1.0], None);
    cache.set("b", vec![2.0], None);
    cache.clear();
    assert_eq!(cache.size(), 0);

    // After clear, can fill up again without premature eviction
    cache.set("x", vec![1.0], None);
    cache.set("y", vec![2.0], None);
    cache.set("z", vec![3.0], None);
    assert_eq!(cache.size(), 3);
}

#[test]
fn test_lru_serialization_preserves_max_entries() {
    let mut cache = EmbeddingCache::with_max_entries("test", 500);
    cache.set("a", vec![1.0], None);

    let dict = cache.to_dict();
    let restored = EmbeddingCache::from_dict(&dict);
    assert_eq!(restored.max_entries, 500);
}

#[test]
fn test_embedding_cache_config_default() {
    let config = EmbeddingCacheConfig::default();
    assert_eq!(config.model, "text-embedding-3-small");
    assert_eq!(config.max_entries, DEFAULT_MAX_ENTRIES);
}
