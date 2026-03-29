//! Symbol cache for LSP results.
//!
//! Caches workspace symbol queries to avoid repeated LSP calls. Uses JSON
//! serialization with versioned entries and TTL-based expiration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::protocol::UnifiedSymbolInfo;

/// Default cache TTL: 5 minutes.
const DEFAULT_TTL_SECS: u64 = 300;

/// Cache format version — bump to invalidate old caches.
const CACHE_VERSION: u32 = 1;

/// A cached symbol query result.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// Symbols returned by the query.
    symbols: Vec<UnifiedSymbolInfo>,
    /// When this entry was created (epoch seconds).
    created_at: u64,
    /// Cache format version.
    version: u32,
}

impl CacheEntry {
    fn is_expired(&self, ttl: Duration) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.created_at) >= ttl.as_secs()
    }

    fn is_current_version(&self) -> bool {
        self.version == CACHE_VERSION
    }
}

/// In-memory + disk symbol cache.
pub struct SymbolCache {
    /// In-memory cache keyed by (workspace_root, query).
    memory: HashMap<(PathBuf, String), CacheEntry>,
    /// Directory for persisted cache files.
    cache_dir: Option<PathBuf>,
    /// TTL for cache entries.
    ttl: Duration,
}

impl SymbolCache {
    /// Create a new symbol cache.
    pub fn new(cache_dir: Option<PathBuf>, ttl_secs: Option<u64>) -> Self {
        let ttl = Duration::from_secs(ttl_secs.unwrap_or(DEFAULT_TTL_SECS));

        if let Some(ref dir) = cache_dir
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            warn!("Failed to create cache dir {}: {}", dir.display(), e);
        }

        Self {
            memory: HashMap::new(),
            cache_dir,
            ttl,
        }
    }

    /// Get cached symbols for a workspace + query, if not expired.
    pub fn get(&mut self, workspace: &Path, query: &str) -> Option<Vec<UnifiedSymbolInfo>> {
        let key = (workspace.to_path_buf(), query.to_string());

        // Check in-memory first
        if let Some(entry) = self.memory.get(&key) {
            if entry.is_current_version() && !entry.is_expired(self.ttl) {
                debug!("Symbol cache hit (memory): {:?}", key);
                return Some(entry.symbols.clone());
            }
            // Expired or wrong version — remove
            self.memory.remove(&key);
        }

        // Check disk cache
        if let Some(entry) = self.load_from_disk(workspace, query)
            && entry.is_current_version()
            && !entry.is_expired(self.ttl)
        {
            debug!("Symbol cache hit (disk): {:?}", key);
            let symbols = entry.symbols.clone();
            self.memory.insert(key, entry);
            return Some(symbols);
        }

        None
    }

    /// Store symbols in cache.
    pub fn put(&mut self, workspace: &Path, query: &str, symbols: Vec<UnifiedSymbolInfo>) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = CacheEntry {
            symbols,
            created_at: now,
            version: CACHE_VERSION,
        };

        let key = (workspace.to_path_buf(), query.to_string());
        self.save_to_disk(workspace, query, &entry);
        self.memory.insert(key, entry);
    }

    /// Invalidate all entries for a workspace.
    pub fn invalidate_workspace(&mut self, workspace: &Path) {
        self.memory.retain(|(ws, _), _| ws != workspace);

        if let Some(ref cache_dir) = self.cache_dir {
            let ws_cache = self.workspace_cache_dir(cache_dir, workspace);
            if ws_cache.exists()
                && let Err(e) = std::fs::remove_dir_all(&ws_cache)
            {
                warn!("Failed to remove cache dir {}: {}", ws_cache.display(), e);
            }
        }
    }

    /// Clear all cached data.
    pub fn clear(&mut self) {
        self.memory.clear();
        if let Some(ref cache_dir) = self.cache_dir {
            if let Err(e) = std::fs::remove_dir_all(cache_dir) {
                warn!("Failed to clear cache dir: {}", e);
            }
            let _ = std::fs::create_dir_all(cache_dir);
        }
    }

    fn cache_key(query: &str) -> String {
        // Simple hash to avoid filesystem issues with query strings
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        query.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn workspace_cache_dir(&self, cache_dir: &Path, workspace: &Path) -> PathBuf {
        let ws_hash = Self::cache_key(&workspace.display().to_string());
        cache_dir.join(ws_hash)
    }

    fn load_from_disk(&self, workspace: &Path, query: &str) -> Option<CacheEntry> {
        let cache_dir = self.cache_dir.as_ref()?;
        let ws_dir = self.workspace_cache_dir(cache_dir, workspace);
        let file = ws_dir.join(format!("{}.json", Self::cache_key(query)));

        let content = std::fs::read_to_string(&file).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn save_to_disk(&self, workspace: &Path, query: &str, entry: &CacheEntry) {
        let cache_dir = match &self.cache_dir {
            Some(d) => d,
            None => return,
        };

        let ws_dir = self.workspace_cache_dir(cache_dir, workspace);
        if let Err(e) = std::fs::create_dir_all(&ws_dir) {
            warn!("Failed to create cache subdir: {}", e);
            return;
        }

        let file = ws_dir.join(format!("{}.json", Self::cache_key(query)));
        match serde_json::to_string(entry) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&file, content) {
                    warn!("Failed to write cache file: {}", e);
                }
            }
            Err(e) => warn!("Failed to serialize cache entry: {}", e),
        }
    }
}

impl std::fmt::Debug for SymbolCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymbolCache")
            .field("entries", &self.memory.len())
            .field("cache_dir", &self.cache_dir)
            .field("ttl", &self.ttl)
            .finish()
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
