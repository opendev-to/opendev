use super::*;

#[test]
fn test_tokenize() {
    let tokens = tokenize("Hello, world! This is a test.");
    assert!(tokens.contains(&"hello".to_string()));
    assert!(tokens.contains(&"world".to_string()));
    assert!(tokens.contains(&"this".to_string()));
    assert!(tokens.contains(&"test".to_string()));
    // Single-char words are filtered out
    assert!(!tokens.contains(&"a".to_string()));
}

#[test]
fn test_tokenize_empty() {
    let tokens = tokenize("");
    assert!(tokens.is_empty());
}

#[test]
fn test_normalize() {
    let mut vec = vec![3.0, 4.0];
    normalize(&mut vec);
    let norm: f64 = vec.iter().map(|x| x * x).sum::<f64>().sqrt();
    assert!((norm - 1.0).abs() < 1e-10);
}

#[test]
fn test_normalize_zero_vector() {
    let mut vec = vec![0.0, 0.0, 0.0];
    normalize(&mut vec);
    assert_eq!(vec, vec![0.0, 0.0, 0.0]);
}

#[test]
fn test_default_embedder() {
    let embedder = TfIdfEmbedder::default_embedder();
    assert_eq!(embedder.dimension(), DEFAULT_DIM);

    let emb = embedder.embed("hello world");
    assert_eq!(emb.len(), DEFAULT_DIM);

    // Should be normalized
    let norm: f64 = emb.iter().map(|x| x * x).sum::<f64>().sqrt();
    assert!((norm - 1.0).abs() < 1e-10);
}

#[test]
fn test_default_embedder_empty_text() {
    let embedder = TfIdfEmbedder::default_embedder();
    let emb = embedder.embed("");
    assert_eq!(emb.len(), DEFAULT_DIM);
    // All zeros since no tokens
    assert!(emb.iter().all(|&x| x == 0.0));
}

#[test]
fn test_tfidf_embedder_with_seeds() {
    let seeds = &[
        "rust programming language systems",
        "python scripting language dynamic",
        "rust cargo build system package",
    ];
    let embedder = TfIdfEmbedder::new(seeds, 50);

    let emb1 = embedder.embed("rust programming");
    let emb2 = embedder.embed("python scripting");

    assert_eq!(emb1.len(), embedder.dimension());
    assert_eq!(emb2.len(), embedder.dimension());

    // Embeddings for different texts should differ
    assert_ne!(emb1, emb2);
}

#[test]
fn test_tfidf_similar_texts_closer() {
    let seeds = &[
        "rust programming language",
        "python scripting language",
        "cooking recipes food",
        "rust cargo build tools",
    ];
    let embedder = TfIdfEmbedder::new(seeds, 100);

    let rust1 = embedder.embed("rust programming cargo");
    let rust2 = embedder.embed("rust language build");
    let cooking = embedder.embed("cooking food recipes");

    let sim_rust = cosine_sim(&rust1, &rust2);
    let sim_diff = cosine_sim(&rust1, &cooking);

    assert!(
        sim_rust > sim_diff,
        "similar topics should be closer: rust-rust={sim_rust} vs rust-cooking={sim_diff}"
    );
}

#[test]
fn test_embed_batch() {
    let embedder = TfIdfEmbedder::default_embedder();
    let texts = &["hello world", "goodbye world", "foo bar"];
    let embeddings = embedder.embed_batch(texts);
    assert_eq!(embeddings.len(), 3);
    for emb in &embeddings {
        assert_eq!(emb.len(), DEFAULT_DIM);
    }
}

#[test]
fn test_deterministic_embeddings() {
    let embedder = TfIdfEmbedder::default_embedder();
    let emb1 = embedder.embed("consistent output");
    let emb2 = embedder.embed("consistent output");
    assert_eq!(emb1, emb2);
}

#[test]
fn test_simple_hash_deterministic() {
    let h1 = simple_hash("test");
    let h2 = simple_hash("test");
    assert_eq!(h1, h2);
}

#[test]
fn test_simple_hash_different() {
    let h1 = simple_hash("abc");
    let h2 = simple_hash("xyz");
    assert_ne!(h1, h2);
}

/// Helper: cosine similarity between two vectors.
fn cosine_sim(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na * nb)
}
