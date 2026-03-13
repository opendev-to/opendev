//! Models.dev API cache and model/provider registry.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tracing::{debug, warn};

const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 24); // 24 hours

/// Display order for provider lists.
const PRIORITY_PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "fireworks",
    "fireworks-ai",
    "google",
    "deepseek",
    "groq",
    "mistral",
    "cohere",
    "perplexity",
    "togetherai",
    "together",
];

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("failed to read cache: {0}")]
    CacheRead(#[from] std::io::Error),
    #[error("failed to parse JSON: {0}")]
    JsonParse(#[from] serde_json::Error),
    #[error("network fetch failed: {0}")]
    NetworkError(String),
}

/// Information about a specific model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub context_length: u64,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub pricing_input: f64,
    #[serde(default)]
    pub pricing_output: f64,
    #[serde(default = "default_pricing_unit")]
    pub pricing_unit: String,
    #[serde(default)]
    pub serverless: bool,
    #[serde(default)]
    pub tunable: bool,
    #[serde(default)]
    pub recommended: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(default = "default_true")]
    pub supports_temperature: bool,
    #[serde(default = "default_api_type")]
    pub api_type: String,
}

fn default_pricing_unit() -> String {
    "per million tokens".to_string()
}
fn default_true() -> bool {
    true
}
fn default_api_type() -> String {
    "chat".to_string()
}

impl ModelInfo {
    /// Format pricing for display.
    pub fn format_pricing(&self) -> String {
        if self.pricing_input == 0.0 && self.pricing_output == 0.0 {
            return "N/A".to_string();
        }
        format!(
            "${:.2} in / ${:.2} out {}",
            self.pricing_input, self.pricing_output, self.pricing_unit
        )
    }
}

impl std::fmt::Display for ModelInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let caps = self.capabilities.join(", ");
        write!(
            f,
            "{}\n  Provider: {}\n  Context: {} tokens\n  Capabilities: {}",
            self.name, self.provider, self.context_length, caps
        )
    }
}

/// Information about a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub api_key_env: String,
    pub api_base_url: String,
    pub models: HashMap<String, ModelInfo>,
}

impl ProviderInfo {
    /// List all models, optionally filtered by capability.
    pub fn list_models(&self, capability: Option<&str>) -> Vec<&ModelInfo> {
        let mut models: Vec<&ModelInfo> = self.models.values().collect();
        if let Some(cap) = capability {
            models.retain(|m| m.capabilities.contains(&cap.to_string()));
        }
        models.sort_by(|a, b| b.context_length.cmp(&a.context_length));
        models
    }

    /// Get the recommended model for this provider.
    pub fn get_recommended_model(&self) -> Option<&ModelInfo> {
        self.models
            .values()
            .find(|m| m.recommended)
            .or_else(|| self.models.values().next())
    }
}

/// Sort key for providers: priority providers first (in order), then alphabetical.
fn provider_sort_key(provider_id: &str) -> (u8, usize, String) {
    if let Some(idx) = PRIORITY_PROVIDERS.iter().position(|&p| p == provider_id) {
        (0, idx, String::new())
    } else {
        (1, 0, provider_id.to_lowercase())
    }
}

/// Registry for managing model and provider configurations.
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    pub providers: HashMap<String, ProviderInfo>,
}

impl ModelRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Load registry from cache directory.
    pub fn load_from_cache(cache_dir: &Path) -> Self {
        let mut registry = Self::new();
        let providers_dir = cache_dir.join("providers");

        if !registry.load_providers_from_dir(&providers_dir) {
            // Cache empty — try sync
            if let Err(e) = sync_provider_cache(Some(cache_dir), None) {
                warn!("Failed to sync provider cache: {}", e);
            }
            registry.load_providers_from_dir(&providers_dir);
        }

        if registry.providers.is_empty() {
            warn!(
                "No provider configurations loaded. \
                 Check network connectivity and retry, or run: opendev setup"
            );
        }

        // Schedule background refresh if stale
        if !registry.providers.is_empty() && is_cache_stale(&providers_dir, DEFAULT_CACHE_TTL) {
            let cache_dir = cache_dir.to_path_buf();
            // Use tokio::spawn if inside a runtime, otherwise fall back to a thread
            if let Ok(_handle) = tokio::runtime::Handle::try_current() {
                let cache_dir_clone = cache_dir.clone();
                tokio::spawn(async move {
                    let _ = sync_provider_cache_async(Some(&cache_dir_clone), None).await;
                });
            } else {
                std::thread::Builder::new()
                    .name("models-dev-sync".to_string())
                    .spawn(move || {
                        let _ = sync_provider_cache(Some(&cache_dir), None);
                    })
                    .ok();
            }
        }

        registry
    }

    /// Load all provider JSON files from a directory.
    fn load_providers_from_dir(&mut self, directory: &Path) -> bool {
        if !directory.exists() {
            return false;
        }

        let mut count = 0;
        let mut entries: Vec<_> = match std::fs::read_dir(directory) {
            Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
            Err(_) => return false,
        };
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
            {
                continue;
            }

            match Self::load_provider_file(&path) {
                Ok(provider) => {
                    self.providers.insert(provider.id.clone(), provider);
                    count += 1;
                }
                Err(e) => {
                    debug!("Failed to load provider {:?}: {}", path.file_name(), e);
                }
            }
        }

        count > 0
    }

    /// Load a single provider JSON file.
    fn load_provider_file(path: &Path) -> Result<ProviderInfo, RegistryError> {
        let content = std::fs::read_to_string(path)?;
        let data: serde_json::Value = serde_json::from_str(&content)?;

        let provider_id = data["id"].as_str().unwrap_or_default().to_string();

        let mut models = HashMap::new();
        if let Some(models_obj) = data["models"].as_object() {
            for (model_key, model_data) in models_obj {
                let pricing = model_data.get("pricing").cloned().unwrap_or_default();
                models.insert(
                    model_key.clone(),
                    ModelInfo {
                        id: model_data["id"].as_str().unwrap_or(model_key).to_string(),
                        name: model_data["name"].as_str().unwrap_or(model_key).to_string(),
                        provider: model_data["provider"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        context_length: model_data["context_length"].as_u64().unwrap_or(0),
                        capabilities: model_data["capabilities"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        pricing_input: pricing["input"].as_f64().unwrap_or(0.0),
                        pricing_output: pricing["output"].as_f64().unwrap_or(0.0),
                        pricing_unit: pricing["unit"]
                            .as_str()
                            .unwrap_or("per million tokens")
                            .to_string(),
                        serverless: model_data["serverless"].as_bool().unwrap_or(false),
                        tunable: model_data["tunable"].as_bool().unwrap_or(false),
                        recommended: model_data["recommended"].as_bool().unwrap_or(false),
                        max_tokens: model_data["max_tokens"].as_u64(),
                        supports_temperature: model_data["supports_temperature"]
                            .as_bool()
                            .unwrap_or(true),
                        api_type: model_data["api_type"]
                            .as_str()
                            .unwrap_or("chat")
                            .to_string(),
                    },
                );
            }
        }

        Ok(ProviderInfo {
            id: provider_id,
            name: data["name"].as_str().unwrap_or_default().to_string(),
            description: data["description"].as_str().unwrap_or_default().to_string(),
            api_key_env: data["api_key_env"].as_str().unwrap_or_default().to_string(),
            api_base_url: data["api_base_url"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            models,
        })
    }

    /// Get provider information by ID.
    pub fn get_provider(&self, provider_id: &str) -> Option<&ProviderInfo> {
        self.providers.get(provider_id)
    }

    /// List all available providers, sorted by priority then alphabetically.
    pub fn list_providers(&self) -> Vec<&ProviderInfo> {
        let mut providers: Vec<&ProviderInfo> = self.providers.values().collect();
        providers.sort_by(|a, b| provider_sort_key(&a.id).cmp(&provider_sort_key(&b.id)));
        providers
    }

    /// Get model information by provider and model key.
    pub fn get_model(&self, provider_id: &str, model_key: &str) -> Option<&ModelInfo> {
        self.providers
            .get(provider_id)
            .and_then(|p| p.models.get(model_key))
    }

    /// Find a model by its full ID across all providers.
    pub fn find_model_by_id(&self, model_id: &str) -> Option<(&str, &str, &ModelInfo)> {
        for (provider_id, provider) in &self.providers {
            for (model_key, model) in &provider.models {
                if model.id == model_id {
                    return Some((provider_id, model_key, model));
                }
            }
        }
        None
    }

    /// List all models across all providers with optional filters.
    pub fn list_all_models(
        &self,
        capability: Option<&str>,
        max_price: Option<f64>,
    ) -> Vec<(&str, &ModelInfo)> {
        let mut models = Vec::new();
        for (provider_id, provider) in &self.providers {
            for model in provider.models.values() {
                if let Some(cap) = capability
                    && !model.capabilities.contains(&cap.to_string())
                {
                    continue;
                }
                if let Some(max) = max_price
                    && model.pricing_output > max
                {
                    continue;
                }
                models.push((provider_id.as_str(), model));
            }
        }
        models.sort_by(|a, b| a.1.pricing_output.partial_cmp(&b.1.pricing_output).unwrap());
        models
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if the per-provider cache needs refreshing.
pub fn is_cache_stale(providers_dir: &Path, ttl: Duration) -> bool {
    let marker = providers_dir.join(".last_sync");
    if !marker.exists() {
        return true;
    }
    match marker.metadata().and_then(|m| m.modified()) {
        Ok(mtime) => SystemTime::now()
            .duration_since(mtime)
            .map_or(true, |age| age > ttl),
        Err(_) => true,
    }
}

/// Fetch models.dev and write per-provider JSON files to cache.
pub fn sync_provider_cache(
    cache_dir: Option<&Path>,
    cache_ttl: Option<Duration>,
) -> Result<bool, RegistryError> {
    let cache_dir = cache_dir.map(PathBuf::from).unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".opendev")
            .join("cache")
    });
    let providers_dir = cache_dir.join("providers");
    let ttl = cache_ttl.unwrap_or(DEFAULT_CACHE_TTL);

    // Check TTL via marker file
    let marker = providers_dir.join(".last_sync");
    if marker.exists()
        && let Ok(meta) = marker.metadata()
        && let Ok(mtime) = meta.modified()
        && SystemTime::now()
            .duration_since(mtime)
            .is_ok_and(|age| age <= ttl)
    {
        return Ok(false); // Still fresh
    }

    // Respect env overrides
    if std::env::var("OPENDEV_DISABLE_REMOTE_MODELS")
        .is_ok_and(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
    {
        return Ok(false);
    }

    // Support OPENDEV_MODELS_DEV_PATH override
    let catalog = if let Ok(override_path) = std::env::var("OPENDEV_MODELS_DEV_PATH") {
        let path = PathBuf::from(override_path);
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            serde_json::from_str(&content)?
        } else {
            warn!("OPENDEV_MODELS_DEV_PATH {:?} does not exist", path);
            return Ok(false);
        }
    } else {
        match fetch_models_dev() {
            Some(data) => data,
            None => return Ok(false),
        }
    };

    std::fs::create_dir_all(&providers_dir)?;

    if let Some(catalog_obj) = catalog.as_object() {
        for (provider_id, provider_data) in catalog_obj {
            if let Some(converted) = convert_provider_to_internal(provider_id, provider_data) {
                if converted["models"].as_object().is_none_or(|m| m.is_empty()) {
                    continue;
                }
                let path = providers_dir.join(format!("{provider_id}.json"));
                let json = serde_json::to_string_pretty(&converted).unwrap_or_default();
                if let Err(e) = std::fs::write(&path, json) {
                    debug!("Failed to write cache for provider {}: {}", provider_id, e);
                }
            }
        }
    }

    // Touch marker
    let _ = std::fs::File::create(&marker);

    Ok(true)
}

/// Fetch the models.dev catalog asynchronously.
async fn fetch_models_dev_async() -> Option<serde_json::Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(
            std::env::var("OPENDEV_HTTP_USER_AGENT")
                .unwrap_or_else(|_| "opendev-rust/0.1.0".to_string()),
        )
        .build()
        .ok()?;

    match client.get(MODELS_DEV_URL).send().await {
        Ok(resp) if resp.status().is_success() => resp.json().await.ok(),
        Ok(resp) => {
            debug!("models.dev returned status {}", resp.status());
            None
        }
        Err(e) => {
            debug!("Failed to fetch models.dev catalog: {}", e);
            None
        }
    }
}

/// Fetch the models.dev catalog (sync wrapper for non-async contexts).
///
/// Spawns a new tokio runtime if needed. Prefer `fetch_models_dev_async`
/// when already inside an async context.
fn fetch_models_dev() -> Option<serde_json::Value> {
    // Try to use an existing tokio runtime; fall back to creating one.
    match tokio::runtime::Handle::try_current() {
        Ok(_handle) => {
            // We are inside a tokio runtime but on a blocking thread
            // (e.g., spawned via std::thread). Use block_on from a
            // spawn_blocking context or a new thread.
            std::thread::scope(|s| {
                s.spawn(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .ok()
                        .and_then(|rt| rt.block_on(fetch_models_dev_async()))
                })
                .join()
                .ok()
                .flatten()
            })
        }
        Err(_) => {
            // No runtime — build a lightweight one.
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()
                .and_then(|rt| rt.block_on(fetch_models_dev_async()))
        }
    }
}

/// Async version of `sync_provider_cache` for use in async contexts.
///
/// Fetches from models.dev without blocking the tokio runtime.
pub async fn sync_provider_cache_async(
    cache_dir: Option<&Path>,
    cache_ttl: Option<Duration>,
) -> Result<bool, RegistryError> {
    let cache_dir = cache_dir.map(PathBuf::from).unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".opendev")
            .join("cache")
    });
    let providers_dir = cache_dir.join("providers");
    let ttl = cache_ttl.unwrap_or(DEFAULT_CACHE_TTL);

    // Check TTL via marker file
    let marker = providers_dir.join(".last_sync");
    if marker.exists()
        && let Ok(meta) = marker.metadata()
        && let Ok(mtime) = meta.modified()
        && SystemTime::now()
            .duration_since(mtime)
            .is_ok_and(|age| age <= ttl)
    {
        return Ok(false); // Still fresh
    }

    if std::env::var("OPENDEV_DISABLE_REMOTE_MODELS")
        .is_ok_and(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
    {
        return Ok(false);
    }

    let catalog = if let Ok(override_path) = std::env::var("OPENDEV_MODELS_DEV_PATH") {
        let path = PathBuf::from(override_path);
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            serde_json::from_str(&content)?
        } else {
            warn!("OPENDEV_MODELS_DEV_PATH {:?} does not exist", path);
            return Ok(false);
        }
    } else {
        match fetch_models_dev_async().await {
            Some(data) => data,
            None => return Ok(false),
        }
    };

    std::fs::create_dir_all(&providers_dir)?;

    if let Some(catalog_obj) = catalog.as_object() {
        for (provider_id, provider_data) in catalog_obj {
            if let Some(converted) = convert_provider_to_internal(provider_id, provider_data) {
                if converted["models"].as_object().is_none_or(|m| m.is_empty()) {
                    continue;
                }
                let path = providers_dir.join(format!("{provider_id}.json"));
                let json = serde_json::to_string_pretty(&converted).unwrap_or_default();
                if let Err(e) = std::fs::write(&path, json) {
                    debug!("Failed to write cache for provider {}: {}", provider_id, e);
                }
            }
        }
    }

    let _ = std::fs::File::create(&marker);
    Ok(true)
}

/// Convert a models.dev provider entry to internal JSON format.
fn convert_provider_to_internal(
    provider_id: &str,
    provider_data: &serde_json::Value,
) -> Option<serde_json::Value> {
    let provider_name = provider_data["name"].as_str().unwrap_or(provider_id);

    let env_vars = provider_data["env"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let models_block = provider_data["models"].as_object()?;

    let mut converted_models = serde_json::Map::new();
    let mut first_model = true;

    for (model_key, md) in models_block {
        let limit = md.get("limit").cloned().unwrap_or_default();
        let context = limit["context"].as_u64().unwrap_or(0);
        if context == 0 {
            continue;
        }

        let modalities = md.get("modalities").cloned().unwrap_or_default();
        let input_mods: Vec<String> = modalities["input"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        if !input_mods.is_empty() && !input_mods.contains(&"text".to_string()) {
            continue; // Skip embedding-only / non-text models
        }

        let cost = md.get("cost").cloned().unwrap_or_default();
        let mut capabilities = Vec::new();
        if input_mods.is_empty() || input_mods.contains(&"text".to_string()) {
            capabilities.push("text");
        }
        if input_mods.contains(&"image".to_string()) {
            capabilities.push("vision");
        }
        if md["reasoning"].as_bool().unwrap_or(false) {
            capabilities.push("reasoning");
        }
        if input_mods.contains(&"audio".to_string()) {
            capabilities.push("audio");
        }

        let max_tokens_raw = limit["output"].as_u64().unwrap_or(0);

        converted_models.insert(
            model_key.clone(),
            serde_json::json!({
                "id": md["id"].as_str().unwrap_or(model_key),
                "name": md["name"].as_str().unwrap_or(model_key),
                "provider": provider_name,
                "context_length": context,
                "capabilities": capabilities,
                "pricing": {
                    "input": cost["input"].as_f64().unwrap_or(0.0),
                    "output": cost["output"].as_f64().unwrap_or(0.0),
                    "unit": "per 1M tokens",
                },
                "recommended": first_model,
                "max_tokens": if max_tokens_raw > 0 { serde_json::Value::Number(max_tokens_raw.into()) } else { serde_json::Value::Null },
                "supports_temperature": md.get("temperature").and_then(|v| v.as_bool()).unwrap_or(true),
            }),
        );
        first_model = false;
    }

    Some(serde_json::json!({
        "id": provider_id,
        "name": provider_name,
        "description": format!("{} models", provider_name),
        "api_key_env": env_vars.first().unwrap_or(&String::new()),
        "api_base_url": provider_data["api"].as_str().unwrap_or(""),
        "models": converted_models,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info_display() {
        let model = ModelInfo {
            id: "gpt-4".to_string(),
            name: "GPT-4".to_string(),
            provider: "OpenAI".to_string(),
            context_length: 128_000,
            capabilities: vec!["text".to_string(), "vision".to_string()],
            pricing_input: 30.0,
            pricing_output: 60.0,
            pricing_unit: "per million tokens".to_string(),
            serverless: false,
            tunable: false,
            recommended: true,
            max_tokens: Some(4096),
            supports_temperature: true,
            api_type: "chat".to_string(),
        };
        let display = format!("{}", model);
        assert!(display.contains("GPT-4"));
        assert!(display.contains("128000"));
    }

    #[test]
    fn test_model_info_pricing() {
        let model = ModelInfo {
            id: "test".to_string(),
            name: "Test".to_string(),
            provider: "Test".to_string(),
            context_length: 4096,
            capabilities: vec![],
            pricing_input: 1.5,
            pricing_output: 2.0,
            pricing_unit: "per million tokens".to_string(),
            serverless: false,
            tunable: false,
            recommended: false,
            max_tokens: None,
            supports_temperature: true,
            api_type: "chat".to_string(),
        };
        assert_eq!(
            model.format_pricing(),
            "$1.50 in / $2.00 out per million tokens"
        );

        let free = ModelInfo {
            pricing_input: 0.0,
            pricing_output: 0.0,
            ..model.clone()
        };
        assert_eq!(free.format_pricing(), "N/A");
    }

    #[test]
    fn test_provider_list_models() {
        let mut models = HashMap::new();
        models.insert(
            "small".to_string(),
            ModelInfo {
                id: "small".to_string(),
                name: "Small".to_string(),
                provider: "Test".to_string(),
                context_length: 4096,
                capabilities: vec!["text".to_string()],
                pricing_input: 0.0,
                pricing_output: 0.0,
                pricing_unit: "per million tokens".to_string(),
                serverless: false,
                tunable: false,
                recommended: false,
                max_tokens: None,
                supports_temperature: true,
                api_type: "chat".to_string(),
            },
        );
        models.insert(
            "large".to_string(),
            ModelInfo {
                id: "large".to_string(),
                name: "Large".to_string(),
                provider: "Test".to_string(),
                context_length: 128_000,
                capabilities: vec!["text".to_string(), "vision".to_string()],
                pricing_input: 0.0,
                pricing_output: 0.0,
                pricing_unit: "per million tokens".to_string(),
                serverless: false,
                tunable: false,
                recommended: true,
                max_tokens: None,
                supports_temperature: true,
                api_type: "chat".to_string(),
            },
        );

        let provider = ProviderInfo {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Test provider".to_string(),
            api_key_env: "TEST_API_KEY".to_string(),
            api_base_url: "https://api.test.com".to_string(),
            models,
        };

        let all = provider.list_models(None);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].context_length, 128_000); // sorted by context desc

        let vision = provider.list_models(Some("vision"));
        assert_eq!(vision.len(), 1);
        assert_eq!(vision[0].id, "large");

        assert_eq!(provider.get_recommended_model().unwrap().id, "large");
    }

    #[test]
    fn test_registry_from_cache() {
        let tmp = tempfile::TempDir::new().unwrap();
        let providers_dir = tmp.path().join("providers");
        std::fs::create_dir_all(&providers_dir).unwrap();

        let provider_json = serde_json::json!({
            "id": "test-provider",
            "name": "Test Provider",
            "description": "A test provider",
            "api_key_env": "TEST_KEY",
            "api_base_url": "https://api.test.com",
            "models": {
                "model-1": {
                    "id": "model-1",
                    "name": "Model One",
                    "provider": "Test Provider",
                    "context_length": 4096,
                    "capabilities": ["text"],
                    "pricing": {"input": 1.0, "output": 2.0, "unit": "per 1M tokens"},
                    "recommended": true
                }
            }
        });

        std::fs::write(
            providers_dir.join("test-provider.json"),
            serde_json::to_string_pretty(&provider_json).unwrap(),
        )
        .unwrap();

        let mut registry = ModelRegistry::new();
        assert!(registry.load_providers_from_dir(&providers_dir));
        assert_eq!(registry.providers.len(), 1);

        let provider = registry.get_provider("test-provider").unwrap();
        assert_eq!(provider.name, "Test Provider");
        assert_eq!(provider.models.len(), 1);

        let model = registry.get_model("test-provider", "model-1").unwrap();
        assert_eq!(model.context_length, 4096);

        let found = registry.find_model_by_id("model-1").unwrap();
        assert_eq!(found.0, "test-provider");
    }

    #[test]
    fn test_convert_provider_to_internal() {
        let provider_data = serde_json::json!({
            "name": "TestAI",
            "env": ["TESTAI_API_KEY"],
            "api": "https://api.testai.com/v1",
            "models": {
                "test-model": {
                    "id": "test-model",
                    "name": "Test Model",
                    "limit": {"context": 8192, "output": 4096},
                    "cost": {"input": 0.5, "output": 1.0},
                    "modalities": {"input": ["text", "image"]},
                    "reasoning": false
                }
            }
        });

        let result = convert_provider_to_internal("testai", &provider_data).unwrap();
        assert_eq!(result["id"], "testai");
        assert_eq!(result["name"], "TestAI");
        assert_eq!(result["api_key_env"], "TESTAI_API_KEY");

        let model = &result["models"]["test-model"];
        assert_eq!(model["context_length"], 8192);
        assert!(
            model["capabilities"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("vision"))
        );
        assert!(model["recommended"].as_bool().unwrap());
    }

    #[test]
    fn test_provider_sort_order() {
        let mut ids = vec!["zebra", "openai", "alpha", "anthropic"];
        ids.sort_by(|a, b| provider_sort_key(a).cmp(&provider_sort_key(b)));
        assert_eq!(ids, vec!["openai", "anthropic", "alpha", "zebra"]);
    }
}
