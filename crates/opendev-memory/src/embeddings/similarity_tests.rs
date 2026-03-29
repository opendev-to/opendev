use super::*;

#[test]
fn test_cosine_similarity_identical() {
    let v = vec![1.0, 0.0, 0.0];
    let sim = cosine_similarity(&v, &v);
    assert!((sim - 1.0).abs() < 1e-10);
}

#[test]
fn test_cosine_similarity_orthogonal() {
    let v1 = vec![1.0, 0.0];
    let v2 = vec![0.0, 1.0];
    let sim = cosine_similarity(&v1, &v2);
    assert!(sim.abs() < 1e-10);
}

#[test]
fn test_cosine_similarity_opposite() {
    let v1 = vec![1.0, 0.0];
    let v2 = vec![-1.0, 0.0];
    let sim = cosine_similarity(&v1, &v2);
    assert!((sim - (-1.0)).abs() < 1e-10);
}

#[test]
fn test_cosine_similarity_zero_vector() {
    let v1 = vec![1.0, 2.0];
    let v2 = vec![0.0, 0.0];
    assert_eq!(cosine_similarity(&v1, &v2), 0.0);
}

#[test]
fn test_cosine_similarity_empty() {
    assert_eq!(cosine_similarity(&[], &[]), 0.0);
}

#[test]
fn test_cosine_similarity_different_lengths() {
    let v1 = vec![1.0, 2.0];
    let v2 = vec![1.0];
    assert_eq!(cosine_similarity(&v1, &v2), 0.0);
}

#[test]
fn test_batch_cosine_similarity() {
    let query = vec![1.0, 0.0];
    let vectors = vec![
        vec![1.0, 0.0],  // identical
        vec![0.0, 1.0],  // orthogonal
        vec![-1.0, 0.0], // opposite
    ];
    let results = batch_cosine_similarity(&query, &vectors);
    assert_eq!(results.len(), 3);
    assert!((results[0] - 1.0).abs() < 1e-10);
    assert!(results[1].abs() < 1e-10);
    assert!((results[2] - (-1.0)).abs() < 1e-10);
}

#[test]
fn test_make_hash_deterministic() {
    let h1 = make_hash("test-model:hello");
    let h2 = make_hash("test-model:hello");
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 16); // 8 bytes = 16 hex chars
}

#[test]
fn test_make_hash_different_inputs() {
    let h1 = make_hash("a");
    let h2 = make_hash("b");
    assert_ne!(h1, h2);
}
