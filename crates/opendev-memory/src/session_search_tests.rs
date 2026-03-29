use super::*;

fn make_sessions() -> Vec<(String, String)> {
    vec![
        (
            "s1".to_string(),
            "rust programming cargo build system".to_string(),
        ),
        (
            "s2".to_string(),
            "python data science machine learning".to_string(),
        ),
        (
            "s3".to_string(),
            "rust async tokio runtime concurrency".to_string(),
        ),
        (
            "s4".to_string(),
            "cooking recipes italian pasta sauce".to_string(),
        ),
    ]
}

#[test]
fn test_semantic_search_basic() {
    let mut cache = EmbeddingCache::new("local");
    let sessions = make_sessions();
    let embedder = TfIdfEmbedder::default_embedder();

    let results =
        semantic_search_sessions("rust cargo build", &mut cache, &sessions, &embedder, 0.0);

    assert!(!results.is_empty());
    // The "rust programming cargo build" session should rank high
    let top_id = &results[0].0;
    assert!(
        top_id == "s1" || top_id == "s3",
        "expected a rust session at top, got {top_id}"
    );
}

#[test]
fn test_semantic_search_empty_query() {
    let mut cache = EmbeddingCache::new("local");
    let sessions = make_sessions();
    let embedder = TfIdfEmbedder::default_embedder();

    let results = semantic_search_sessions("", &mut cache, &sessions, &embedder, 0.0);
    assert!(results.is_empty());
}

#[test]
fn test_semantic_search_empty_sessions() {
    let mut cache = EmbeddingCache::new("local");
    let embedder = TfIdfEmbedder::default_embedder();

    let results = semantic_search_sessions("query", &mut cache, &[], &embedder, 0.0);
    assert!(results.is_empty());
}

#[test]
fn test_semantic_search_min_score_filter() {
    let mut cache = EmbeddingCache::new("local");
    let sessions = make_sessions();
    let embedder = TfIdfEmbedder::default_embedder();

    // With a very high threshold, most results should be filtered out
    let results =
        semantic_search_sessions("rust programming", &mut cache, &sessions, &embedder, 0.99);
    // Exact match is unlikely with TF-IDF, so most should be filtered
    assert!(results.len() <= 1);
}

#[test]
fn test_semantic_search_sorted_descending() {
    let mut cache = EmbeddingCache::new("local");
    let sessions = make_sessions();
    let embedder = TfIdfEmbedder::default_embedder();

    let results =
        semantic_search_sessions("rust programming", &mut cache, &sessions, &embedder, 0.0);

    for window in results.windows(2) {
        assert!(
            window[0].1 >= window[1].1,
            "results should be sorted descending: {} >= {}",
            window[0].1,
            window[1].1
        );
    }
}

#[test]
fn test_semantic_search_uses_cache() {
    let mut cache = EmbeddingCache::new("local");
    let sessions = make_sessions();
    let embedder = TfIdfEmbedder::default_embedder();

    // First search populates the cache
    let results1 = semantic_search_sessions("rust", &mut cache, &sessions, &embedder, 0.0);
    let cache_size_after_first = cache.size();

    // Second search should use cached embeddings
    let results2 = semantic_search_sessions("rust", &mut cache, &sessions, &embedder, 0.0);

    assert_eq!(results1.len(), results2.len());
    assert_eq!(cache.size(), cache_size_after_first);

    // Scores should be identical since embeddings are cached
    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.0, r2.0);
        assert!((r1.1 - r2.1).abs() < 1e-10);
    }
}

#[test]
fn test_semantic_search_default() {
    let mut cache = EmbeddingCache::new("local");
    let sessions = make_sessions();

    let results = semantic_search_sessions_default("rust cargo", &mut cache, &sessions);
    assert!(!results.is_empty());
}

#[test]
fn test_semantic_search_skips_empty_texts() {
    let mut cache = EmbeddingCache::new("local");
    let sessions = vec![
        ("s1".to_string(), "rust programming".to_string()),
        ("s2".to_string(), "".to_string()),
        ("s3".to_string(), "   ".to_string()),
    ];
    let embedder = TfIdfEmbedder::default_embedder();

    let results = semantic_search_sessions("rust", &mut cache, &sessions, &embedder, 0.0);

    // Only s1 should appear (s2 and s3 are empty/whitespace)
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "s1");
}
