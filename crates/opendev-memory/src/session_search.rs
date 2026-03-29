//! Embedding-based semantic search across sessions.
//!
//! Uses the [`EmbeddingCache`] to find sessions whose content is
//! semantically similar to a query string.

use crate::embeddings::{EmbeddingCache, cosine_similarity};
use crate::local_embeddings::{LocalEmbedder, TfIdfEmbedder};

/// A session ID paired with its similarity score.
pub type SessionScore = (String, f64);

/// Search sessions by embedding similarity.
///
/// Given a query string and an [`EmbeddingCache`] containing session
/// embeddings (keyed by session ID), returns sessions ranked by cosine
/// similarity in descending order.
///
/// Each entry in `session_texts` is a `(session_id, text)` pair. The text
/// is typically a concatenation of the session's messages or summary.
///
/// If the query or a session text does not have a cached embedding, one is
/// generated using the provided [`LocalEmbedder`].
///
/// # Arguments
/// * `query` - The search query text.
/// * `cache` - Mutable reference to an embedding cache.
/// * `session_texts` - Pairs of `(session_id, text_content)`.
/// * `embedder` - A local embedder to generate missing embeddings.
/// * `min_score` - Minimum similarity score to include (0.0 to 1.0).
///
/// # Returns
/// A vector of `(session_id, score)` sorted by descending similarity.
pub fn semantic_search_sessions(
    query: &str,
    cache: &mut EmbeddingCache,
    session_texts: &[(String, String)],
    embedder: &dyn LocalEmbedder,
    min_score: f64,
) -> Vec<SessionScore> {
    if query.trim().is_empty() || session_texts.is_empty() {
        return Vec::new();
    }

    // Get or compute query embedding
    let query_embedding = match cache.get(query, None) {
        Some(emb) => emb.clone(),
        None => {
            let emb = embedder.embed(query);
            cache.set(query, emb.clone(), None);
            emb
        }
    };

    let mut results: Vec<SessionScore> = Vec::new();

    for (session_id, text) in session_texts {
        if text.trim().is_empty() {
            continue;
        }

        let session_embedding = match cache.get(text, None) {
            Some(emb) => emb.clone(),
            None => {
                let emb = embedder.embed(text);
                cache.set(text, emb.clone(), None);
                emb
            }
        };

        let score = cosine_similarity(&query_embedding, &session_embedding);
        if score >= min_score {
            results.push((session_id.clone(), score));
        }
    }

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}

/// Convenience wrapper that uses a default [`TfIdfEmbedder`].
///
/// Suitable for quick searches when no pre-trained embedder is available.
pub fn semantic_search_sessions_default(
    query: &str,
    cache: &mut EmbeddingCache,
    session_texts: &[(String, String)],
) -> Vec<SessionScore> {
    let embedder = TfIdfEmbedder::default_embedder();
    semantic_search_sessions(query, cache, session_texts, &embedder, 0.0)
}

#[cfg(test)]
#[path = "session_search_tests.rs"]
mod tests;
