//! Embedding cache and cosine similarity for semantic bullet selection.
//!
//! Mirrors `opendev/core/context_engineering/memory/embeddings.py`.

mod similarity;

pub use similarity::{batch_cosine_similarity, cosine_similarity, make_hash};

use serde::{Deserialize, Serialize};
use similarity::make_key;
use std::collections::HashMap;
use std::path::Path;

/// Default maximum number of entries in the embedding cache.
const DEFAULT_MAX_ENTRIES: usize = 10_000;

/// Metadata for a cached embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingMetadata {
    pub text: String,
    pub model: String,
    pub hash: String,
    pub embedding: Vec<f64>,
}

impl EmbeddingMetadata {
    /// Create embedding metadata with computed hash.
    pub fn create(text: &str, model: &str, embedding: Vec<f64>) -> Self {
        let content = format!("{model}:{text}");
        let hash = make_hash(&content);
        Self {
            text: text.to_string(),
            model: model.to_string(),
            hash,
            embedding,
        }
    }
}

/// Cache for bullet embeddings to avoid redundant API calls.
///
/// Stores embeddings in memory and can be persisted to disk.
/// Cache keys are based on content hash + model name.
/// Uses LRU eviction when the cache exceeds `max_entries`.
#[derive(Debug, Clone)]
pub struct EmbeddingCache {
    pub model: String,
    cache: HashMap<String, EmbeddingMetadata>,
    /// Maximum number of entries before LRU eviction kicks in.
    pub max_entries: usize,
    /// Access order tracking: most recently used keys are at the end.
    access_order: Vec<String>,
}

impl EmbeddingCache {
    /// Create a new embedding cache with the default max entries (10,000).
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            cache: HashMap::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
            access_order: Vec::new(),
        }
    }

    /// Create a new embedding cache with a custom max entries limit.
    pub fn with_max_entries(model: &str, max_entries: usize) -> Self {
        Self {
            model: model.to_string(),
            cache: HashMap::new(),
            max_entries,
            access_order: Vec::new(),
        }
    }

    /// Get cached embedding for text.
    ///
    /// Marks the entry as recently used for LRU tracking.
    pub fn get(&mut self, text: &str, model: Option<&str>) -> Option<&Vec<f64>> {
        let model = model.unwrap_or(&self.model);
        let key = make_key(text, model);
        if self.cache.contains_key(&key) {
            self.touch(&key);
            self.cache.get(&key).map(|meta| &meta.embedding)
        } else {
            None
        }
    }

    /// Get cached embedding without updating LRU order (read-only lookup).
    pub fn peek(&self, text: &str, model: Option<&str>) -> Option<&Vec<f64>> {
        let model = model.unwrap_or(&self.model);
        let key = make_key(text, model);
        self.cache.get(&key).map(|meta| &meta.embedding)
    }

    /// Cache an embedding.
    ///
    /// If the cache is at capacity, the least-recently-used entry is evicted.
    pub fn set(&mut self, text: &str, embedding: Vec<f64>, model: Option<&str>) {
        let model_str = model.unwrap_or(&self.model).to_string();
        let key = make_key(text, &model_str);

        // If key already exists, just update it
        if self.cache.contains_key(&key) {
            let metadata = EmbeddingMetadata::create(text, &model_str, embedding);
            self.cache.insert(key.clone(), metadata);
            self.touch(&key);
            return;
        }

        // Evict LRU entry if at capacity
        if self.max_entries > 0 && self.cache.len() >= self.max_entries {
            self.evict_lru();
        }

        let metadata = EmbeddingMetadata::create(text, &model_str, embedding);
        self.cache.insert(key.clone(), metadata);
        self.access_order.push(key);
    }

    /// Move key to end of access_order (most recently used).
    fn touch(&mut self, key: &str) {
        self.access_order.retain(|k| k != key);
        self.access_order.push(key.to_string());
    }

    /// Evict the least-recently-used entry (front of access_order).
    fn evict_lru(&mut self) {
        if let Some(lru_key) = self.access_order.first().cloned() {
            self.cache.remove(&lru_key);
            self.access_order.remove(0);
        }
    }

    /// Clear all cached embeddings.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.access_order.clear();
    }

    /// Get number of cached embeddings.
    pub fn size(&self) -> usize {
        self.cache.len()
    }

    /// Serialize cache to JSON value.
    pub fn to_dict(&self) -> serde_json::Value {
        let cache_map: serde_json::Map<String, serde_json::Value> = self
            .cache
            .iter()
            .map(|(key, meta)| (key.clone(), serde_json::to_value(meta).unwrap_or_default()))
            .collect();
        serde_json::json!({
            "model": self.model,
            "max_entries": self.max_entries,
            "cache": cache_map,
        })
    }

    /// Deserialize cache from JSON value.
    pub fn from_dict(data: &serde_json::Value) -> Self {
        let model = data
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("text-embedding-3-small")
            .to_string();

        let max_entries = data
            .get("max_entries")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(DEFAULT_MAX_ENTRIES);

        let mut cache = HashMap::new();
        let mut access_order = Vec::new();
        if let Some(cache_obj) = data.get("cache").and_then(|v| v.as_object()) {
            for (key, val) in cache_obj {
                if let Ok(meta) = serde_json::from_value::<EmbeddingMetadata>(val.clone()) {
                    cache.insert(key.clone(), meta);
                    access_order.push(key.clone());
                }
            }
        }

        Self {
            model,
            cache,
            max_entries,
            access_order,
        }
    }

    /// Save cache to JSON file.
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.to_dict()).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Load cache from JSON file. Returns None if file doesn't exist or is corrupt.
    pub fn load_from_file(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let data: serde_json::Value = serde_json::from_str(&content).ok()?;
        Some(Self::from_dict(&data))
    }
}

impl Default for EmbeddingCache {
    fn default() -> Self {
        Self::new("text-embedding-3-small")
    }
}

/// Configuration for EmbeddingCache.
#[derive(Debug, Clone)]
pub struct EmbeddingCacheConfig {
    pub model: String,
    pub max_entries: usize,
}

impl Default for EmbeddingCacheConfig {
    fn default() -> Self {
        Self {
            model: "text-embedding-3-small".to_string(),
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }
}

#[cfg(test)]
mod tests;
