//! Local embedding generation using TF-IDF bag-of-words.
//!
//! Provides a simple local embedder that generates embeddings without
//! requiring an external API. Uses TF-IDF (Term Frequency - Inverse Document
//! Frequency) to produce fixed-dimension embedding vectors.
//!
//! This module defines the `LocalEmbedder` trait interface that could be
//! backed by ONNX runtime when the `ort` feature is available. The default
//! implementation uses TF-IDF for basic local embeddings.

use std::collections::HashMap;

/// Trait for local embedding generation.
///
/// Implementations should produce normalized embedding vectors suitable for
/// cosine similarity comparisons.
pub trait LocalEmbedder: Send + Sync {
    /// Generate an embedding vector for the given text.
    fn embed(&self, text: &str) -> Vec<f64>;

    /// Generate embeddings for multiple texts.
    fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f64>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Return the dimensionality of the embedding vectors.
    fn dimension(&self) -> usize;
}

/// A simple TF-IDF based local embedder.
///
/// Uses a fixed vocabulary built from a set of seed documents. Each text is
/// represented as a normalized TF-IDF vector over the vocabulary.
///
/// This provides reasonable similarity detection for related texts without
/// requiring any external model or API.
#[derive(Debug, Clone)]
pub struct TfIdfEmbedder {
    /// Vocabulary: word -> index mapping.
    vocabulary: HashMap<String, usize>,
    /// Inverse document frequency for each term.
    idf: Vec<f64>,
    /// Dimensionality of output vectors.
    dim: usize,
}

/// Default vocabulary size when no seed documents are provided.
const DEFAULT_DIM: usize = 256;

impl TfIdfEmbedder {
    /// Create a new TF-IDF embedder with a pre-built vocabulary.
    ///
    /// The vocabulary is built from the provided seed documents. Each unique
    /// word (lowercased, alphanumeric only) becomes a dimension in the
    /// embedding space, up to `max_dim` dimensions.
    pub fn new(seed_documents: &[&str], max_dim: usize) -> Self {
        let mut word_doc_count: HashMap<String, usize> = HashMap::new();
        let mut word_freq: HashMap<String, usize> = HashMap::new();
        let total_docs = seed_documents.len().max(1);

        for doc in seed_documents {
            let mut seen_in_doc: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for word in tokenize(doc) {
                *word_freq.entry(word.clone()).or_insert(0) += 1;
                if seen_in_doc.insert(word.clone()) {
                    *word_doc_count.entry(word).or_insert(0) += 1;
                }
            }
        }

        // Select top-N words by frequency as the vocabulary
        let mut words_by_freq: Vec<(String, usize)> = word_freq.into_iter().collect();
        words_by_freq.sort_by(|a, b| b.1.cmp(&a.1));
        words_by_freq.truncate(max_dim);

        let mut vocabulary = HashMap::new();
        let mut idf = Vec::new();
        for (idx, (word, _freq)) in words_by_freq.into_iter().enumerate() {
            let doc_count = word_doc_count.get(&word).copied().unwrap_or(1);
            let idf_value = ((total_docs as f64) / (doc_count as f64 + 1.0)).ln() + 1.0;
            vocabulary.insert(word, idx);
            idf.push(idf_value);
        }

        let dim = vocabulary.len().max(1);

        Self {
            vocabulary,
            idf,
            dim,
        }
    }

    /// Create a TF-IDF embedder with default settings and no seed documents.
    ///
    /// Uses a hash-based approach to map words to dimensions, suitable when
    /// no training corpus is available.
    pub fn default_embedder() -> Self {
        Self {
            vocabulary: HashMap::new(),
            idf: Vec::new(),
            dim: DEFAULT_DIM,
        }
    }
}

impl LocalEmbedder for TfIdfEmbedder {
    fn embed(&self, text: &str) -> Vec<f64> {
        let tokens = tokenize(text);
        let total_tokens = tokens.len().max(1) as f64;

        if self.vocabulary.is_empty() {
            // Hash-based fallback: map words to dimensions via hash
            let mut vec = vec![0.0f64; self.dim];
            for token in &tokens {
                let hash = simple_hash(token) % self.dim;
                vec[hash] += 1.0 / total_tokens;
            }
            normalize(&mut vec);
            return vec;
        }

        // TF-IDF computation
        let mut vec = vec![0.0f64; self.dim];
        for token in &tokens {
            if let Some(&idx) = self.vocabulary.get(token) {
                let tf = 1.0 / total_tokens;
                vec[idx] += tf * self.idf[idx];
            }
        }
        normalize(&mut vec);
        vec
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

impl Default for TfIdfEmbedder {
    fn default() -> Self {
        Self::default_embedder()
    }
}

/// Tokenize text into lowercase alphanumeric words.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_lowercase())
        .collect()
}

/// Simple deterministic hash for a string.
fn simple_hash(s: &str) -> usize {
    let mut hash: usize = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as usize);
    }
    hash
}

/// L2-normalize a vector in place.
fn normalize(vec: &mut [f64]) {
    let norm: f64 = vec.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > 0.0 {
        for x in vec.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
#[path = "local_embeddings_tests.rs"]
mod tests;
