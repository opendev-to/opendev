//! Similarity calculation functions and hashing utilities.

use sha2::{Digest, Sha256};

/// Calculate cosine similarity between two vectors.
///
/// Returns a value between -1.0 and 1.0:
/// - 1.0 = identical direction
/// - 0.0 = orthogonal
/// - -1.0 = opposite direction
pub fn cosine_similarity(vec1: &[f64], vec2: &[f64]) -> f64 {
    if vec1.len() != vec2.len() || vec1.is_empty() {
        return 0.0;
    }

    let dot: f64 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
    let norm1: f64 = vec1.iter().map(|a| a * a).sum::<f64>().sqrt();
    let norm2: f64 = vec2.iter().map(|a| a * a).sum::<f64>().sqrt();

    if norm1 == 0.0 || norm2 == 0.0 {
        return 0.0;
    }

    let similarity = dot / (norm1 * norm2);
    similarity.clamp(-1.0, 1.0)
}

/// Calculate cosine similarity between a query vector and multiple vectors.
pub fn batch_cosine_similarity(query: &[f64], vectors: &[Vec<f64>]) -> Vec<f64> {
    vectors
        .iter()
        .map(|v| cosine_similarity(query, v))
        .collect()
}

/// Create a SHA-256 based cache key (first 16 hex chars).
pub(super) fn make_key(text: &str, model: &str) -> String {
    make_hash(&format!("{model}:{text}"))
}

/// SHA-256 hash truncated to 16 hex chars.
pub fn make_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Inline hex encoding (avoids pulling in the `hex` crate just for this).
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
#[path = "similarity_tests.rs"]
mod tests;
