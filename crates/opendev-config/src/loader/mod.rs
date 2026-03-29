//! Hierarchical config loading.
//!
//! Priority: project settings > user settings > env vars > defaults.

mod env_overrides;
mod templates;
mod validation;

use opendev_models::AppConfig;
use std::path::Path;
use thiserror::Error;
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    ReadError {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse config file {path}: {source}")]
    ParseError {
        path: String,
        source: serde_json::Error,
    },
    #[error("config validation failed: {0}")]
    ValidationError(String),
}

/// Loads and merges configuration from multiple sources.
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration with hierarchical merge.
    ///
    /// Priority: project settings > user settings > env vars > defaults.
    pub fn load(global_settings: &Path, project_settings: &Path) -> Result<AppConfig, ConfigError> {
        let mut config = AppConfig::default();

        // Load user-level settings
        if global_settings.exists() {
            match Self::load_file(global_settings) {
                Ok(user_config) => {
                    Self::warn_unknown_fields(&user_config, &global_settings.display().to_string());
                    config = Self::merge(config, user_config);
                    debug!("Loaded global settings from {:?}", global_settings);
                }
                Err(e) => {
                    warn!("Failed to load global settings: {}", e);
                }
            }
        }

        // Load project-level settings (higher priority)
        if project_settings.exists() {
            match Self::load_file(project_settings) {
                Ok(project_config) => {
                    Self::warn_unknown_fields(
                        &project_config,
                        &project_settings.display().to_string(),
                    );
                    config = Self::merge(config, project_config);
                    debug!("Loaded project settings from {:?}", project_settings);
                }
                Err(e) => {
                    warn!("Failed to load project settings: {}", e);
                }
            }
        }

        // Apply environment variable overrides (highest priority after project)
        Self::apply_env_overrides(&mut config);

        // Validate and warn
        let _ = Self::validate(&config);
        Self::validate_cross_field(&config);

        Ok(config)
    }

    fn load_file(path: &Path) -> Result<serde_json::Value, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::ReadError {
            path: path.display().to_string(),
            source: e,
        })?;

        // Apply template substitutions before parsing
        let config_dir = path.parent().unwrap_or(Path::new("."));
        let content = Self::substitute_templates(&content, config_dir);

        let value: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| ConfigError::ParseError {
                path: path.display().to_string(),
                source: e,
            })?;

        // Apply config migrations if the version is outdated
        let (migrated, changed) = crate::migration::migrate_config(value);
        if changed {
            debug!(
                "Migrated config at {:?} to version {}",
                path,
                crate::migration::CURRENT_CONFIG_VERSION
            );
            // Best-effort write-back of migrated config
            if let Ok(json) = serde_json::to_string_pretty(&migrated) {
                let _ = std::fs::write(path, json);
            }
        }

        Ok(migrated)
    }

    /// Merge a partial JSON config onto an existing AppConfig.
    ///
    /// Most fields are replaced by the override. Array fields like `instructions`
    /// are concatenated and deduplicated (matching OpenCode's merge behavior).
    fn merge(base: AppConfig, overrides: serde_json::Value) -> AppConfig {
        // Extract array fields that should be concatenated before the general merge.
        let base_instructions = base.instructions.clone();
        let base_skill_paths = base.skill_paths.clone();
        let base_skill_urls = base.skill_urls.clone();

        let mut base_value = serde_json::to_value(&base).unwrap_or(serde_json::Value::Null);
        if let (Some(base_obj), Some(override_obj)) =
            (base_value.as_object_mut(), overrides.as_object())
        {
            for (key, value) in override_obj {
                base_obj.insert(key.clone(), value.clone());
            }
        }
        let mut merged: AppConfig = serde_json::from_value(base_value).unwrap_or(base);

        // Concat+deduplicate array fields instead of replacing.
        if let Some(override_obj) = overrides.as_object() {
            if override_obj.contains_key("instructions") && !base_instructions.is_empty() {
                let mut combined = base_instructions;
                for item in &merged.instructions {
                    if !combined.contains(item) {
                        combined.push(item.clone());
                    }
                }
                merged.instructions = combined;
            }
            if override_obj.contains_key("skill_paths") && !base_skill_paths.is_empty() {
                let mut combined = base_skill_paths;
                for item in &merged.skill_paths {
                    if !combined.contains(item) {
                        combined.push(item.clone());
                    }
                }
                merged.skill_paths = combined;
            }
            if override_obj.contains_key("skill_urls") && !base_skill_urls.is_empty() {
                let mut combined = base_skill_urls;
                for item in &merged.skill_urls {
                    if !combined.contains(item) {
                        combined.push(item.clone());
                    }
                }
                merged.skill_urls = combined;
            }
        }

        merged
    }

    /// Save configuration to a settings file.
    ///
    /// Writes the config as pretty-printed JSON. Uses atomic write
    /// (write to temp file then rename) to prevent corruption.
    pub fn save(config: &AppConfig, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ConfigError::ReadError {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        let json = serde_json::to_string_pretty(config).map_err(|e| ConfigError::ParseError {
            path: path.display().to_string(),
            source: e,
        })?;

        // Atomic write: write to .tmp then rename
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json).map_err(|e| ConfigError::ReadError {
            path: tmp_path.display().to_string(),
            source: e,
        })?;
        std::fs::rename(&tmp_path, path).map_err(|e| ConfigError::ReadError {
            path: path.display().to_string(),
            source: e,
        })?;

        debug!("Saved config to {:?}", path);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
