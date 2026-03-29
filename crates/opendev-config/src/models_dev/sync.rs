//! Cache staleness checks and synchronisation with models.dev.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, warn};

use super::models::RegistryError;
use super::{DEFAULT_CACHE_TTL, MODELS_DEV_URL};

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
#[path = "sync_tests.rs"]
mod tests;
