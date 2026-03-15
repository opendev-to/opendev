//! Hierarchical config loading.
//!
//! Priority: project settings > user settings > env vars > defaults.

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

        // Apply environment variable overrides
        Self::apply_env_overrides(&mut config);

        // Validate final configuration
        if let Err(e) = Self::validate(&config) {
            warn!("Config validation: {}", e);
        }

        // Cross-field semantic validation (non-blocking warnings)
        Self::validate_cross_field(&config);

        Ok(config)
    }

    /// Load a config file as a partial JSON value, applying migrations if needed.
    ///
    /// Before parsing, applies template variable substitution:
    /// - `{env:VAR_NAME}` → environment variable value (empty string if unset)
    /// - `{file:path}` → file contents (supports `~/`, relative, and absolute paths)
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

    /// Substitute `{env:VAR}` and `{file:path}` templates in config text.
    ///
    /// This runs before JSON parsing, replacing template tokens with their
    /// resolved values. Follows the same pattern as OpenCode's `substitute()`.
    fn substitute_templates(text: &str, config_dir: &Path) -> String {
        let mut result = text.to_string();

        // Process {env:VAR_NAME} tokens
        while let Some(start) = result.find("{env:") {
            let rest = &result[start + 5..];
            if let Some(end) = rest.find('}') {
                let var_name = &rest[..end];
                let replacement = std::env::var(var_name).unwrap_or_else(|_| {
                    debug!(
                        "Config template {{env:{var_name}}}: variable not set, using empty string"
                    );
                    String::new()
                });
                result.replace_range(start..start + 5 + end + 1, &replacement);
            } else {
                break; // Malformed token, stop processing
            }
        }

        // Process {file:path} tokens
        while let Some(start) = result.find("{file:") {
            let rest = &result[start + 6..];
            if let Some(end) = rest.find('}') {
                let file_path = rest[..end].trim();

                let resolved = if file_path.starts_with("~/") {
                    dirs::home_dir()
                        .map(|h| h.join(&file_path[2..]))
                        .unwrap_or_else(|| std::path::PathBuf::from(file_path))
                } else if Path::new(file_path).is_absolute() {
                    std::path::PathBuf::from(file_path)
                } else {
                    config_dir.join(file_path)
                };

                let replacement = match std::fs::read_to_string(&resolved) {
                    Ok(content) => {
                        let trimmed = content.trim();
                        // JSON-escape the content since it's inside a JSON string
                        serde_json::to_string(trimmed)
                            .map(|s| s[1..s.len() - 1].to_string())
                            .unwrap_or_else(|_| trimmed.to_string())
                    }
                    Err(e) => {
                        debug!(
                            "Config template {{file:{file_path}}}: could not read {}: {e}",
                            resolved.display()
                        );
                        String::new()
                    }
                };

                result.replace_range(start..start + 6 + end + 1, &replacement);
            } else {
                break;
            }
        }

        result
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

    /// Check a JSON config object for unknown fields and emit warnings.
    ///
    /// Compares top-level keys against the known fields of `AppConfig`
    /// (derived by serializing a default instance). This catches typos
    /// like `"modle"` instead of `"model"` that `serde(default)` would
    /// silently ignore.
    ///
    /// Returns the list of unknown field names (empty if all are valid).
    pub fn warn_unknown_fields(config_json: &serde_json::Value, source: &str) -> Vec<String> {
        let known_fields = Self::known_field_names();
        let mut unknown = Vec::new();

        if let Some(obj) = config_json.as_object() {
            for key in obj.keys() {
                if !known_fields.contains(key.as_str()) {
                    unknown.push(key.clone());
                }
            }
        }

        if !unknown.is_empty() {
            // Try to suggest corrections for each unknown field
            let suggestions: Vec<String> = unknown
                .iter()
                .map(|u| {
                    if let Some(suggestion) = Self::closest_field(u, &known_fields) {
                        format!("  - \"{u}\" (did you mean \"{suggestion}\"?)")
                    } else {
                        format!("  - \"{u}\"")
                    }
                })
                .collect();
            warn!(
                "Unknown config fields in {source} (will be ignored):\n{}",
                suggestions.join("\n")
            );
        }

        unknown
    }

    /// Get the set of known top-level field names for `AppConfig`.
    fn known_field_names() -> std::collections::HashSet<&'static str> {
        // These are the serde field names (matching JSON keys).
        // Maintained manually to avoid runtime serialization overhead.
        [
            "model_provider",
            "model",
            "model_thinking",
            "model_thinking_provider",
            "model_vlm",
            "model_vlm_provider",
            "model_critique",
            "model_critique_provider",
            "model_compact",
            "model_compact_provider",
            "api_key",
            "api_base_url",
            "max_tokens",
            "temperature",
            "auto_save_interval",
            "max_context_tokens",
            "verbose",
            "debug_logging",
            "color_scheme",
            "show_token_count",
            "enable_sound",
            "permissions",
            "enable_bash",
            "bash_timeout",
            "auto_mode",
            "operation",
            "max_undo_history",
            "topic_detection",
            "playbook",
            "plan_mode_workflow",
            "plan_mode_explore_agent_count",
            "plan_mode_plan_agent_count",
            "plan_mode_explore_variant",
            "instructions",
            "skill_paths",
            "skill_urls",
            "model_variants",
            "default_agent",
            "agents",
            "formatter",
            "config_version",
        ]
        .into_iter()
        .collect()
    }

    /// Find the closest known field name using edit distance.
    ///
    /// Returns `Some(suggestion)` if the distance is ≤ 3 (likely a typo).
    fn closest_field<'a>(
        unknown: &str,
        known: &std::collections::HashSet<&'a str>,
    ) -> Option<&'a str> {
        let mut best: Option<(&str, usize)> = None;
        for &field in known {
            let dist = Self::edit_distance(unknown, field);
            if dist <= 3 && best.as_ref().is_none_or(|(_, d)| dist < *d) {
                best = Some((field, dist));
            }
        }
        best.map(|(f, _)| f)
    }

    /// Levenshtein edit distance between two strings.
    fn edit_distance(a: &str, b: &str) -> usize {
        let a_bytes = a.as_bytes();
        let b_bytes = b.as_bytes();
        let m = a_bytes.len();
        let n = b_bytes.len();

        let mut prev = (0..=n).collect::<Vec<_>>();
        let mut curr = vec![0; n + 1];

        for i in 1..=m {
            curr[0] = i;
            for j in 1..=n {
                let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                    0
                } else {
                    1
                };
                curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
            }
            std::mem::swap(&mut prev, &mut curr);
        }

        prev[n]
    }

    /// Validate the loaded configuration.
    ///
    /// Checks:
    /// - Model names are non-empty strings
    /// - API keys are non-empty when explicitly set
    /// - Temperature is between 0.0 and 2.0
    /// - Max tokens is positive
    pub fn validate(config: &AppConfig) -> Result<(), ConfigError> {
        let mut errors = Vec::new();

        if config.model.trim().is_empty() {
            errors.push("model name must be a non-empty string".to_string());
        }
        if config.model_provider.trim().is_empty() {
            errors.push("model_provider must be a non-empty string".to_string());
        }
        if let Some(ref key) = config.api_key
            && key.trim().is_empty()
        {
            errors.push("api_key must be non-empty when set".to_string());
        }
        if !(0.0..=2.0).contains(&config.temperature) {
            errors.push(format!(
                "temperature must be between 0.0 and 2.0, got {}",
                config.temperature
            ));
        }
        if config.max_tokens == 0 {
            errors.push("max_tokens must be positive".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::ValidationError(errors.join("; ")))
        }
    }

    /// Validate cross-field relationships and emit warnings for likely mistakes.
    ///
    /// These are non-blocking warnings (the config still loads) but help catch
    /// misconfigurations early. Returns the list of warnings for testing.
    pub fn validate_cross_field(config: &AppConfig) -> Vec<String> {
        let mut warnings = Vec::new();

        // 1. Model-provider pairing: model set without matching provider
        let model_pairs: &[(&str, &Option<String>, &str, &Option<String>)] = &[
            (
                "model_thinking",
                &config.model_thinking,
                "model_thinking_provider",
                &config.model_thinking_provider,
            ),
            (
                "model_vlm",
                &config.model_vlm,
                "model_vlm_provider",
                &config.model_vlm_provider,
            ),
            (
                "model_critique",
                &config.model_critique,
                "model_critique_provider",
                &config.model_critique_provider,
            ),
            (
                "model_compact",
                &config.model_compact,
                "model_compact_provider",
                &config.model_compact_provider,
            ),
        ];
        for (model_key, model_val, provider_key, provider_val) in model_pairs {
            if model_val.is_some() && provider_val.is_none() {
                warnings.push(format!(
                    "{model_key} is set but {provider_key} is not — \
                     will fall back to model_provider (\"{}\")",
                    config.model_provider
                ));
            }
        }

        // 2. Playbook scoring weights should roughly sum to 1.0
        let weights = &config.playbook.scoring_weights;
        let sum = weights.effectiveness + weights.recency + weights.semantic;
        if (sum - 1.0).abs() > 0.1 {
            warnings.push(format!(
                "playbook scoring weights sum to {sum:.2} (expected ~1.0): \
                 effectiveness={}, recency={}, semantic={}",
                weights.effectiveness, weights.recency, weights.semantic
            ));
        }

        // 3. Plan mode agent counts
        if config.plan_mode_explore_agent_count == 0 {
            warnings.push(
                "plan_mode_explore_agent_count is 0 — plan mode exploration will be skipped"
                    .to_string(),
            );
        }

        // 4. Auto mode + bash disabled conflict
        if config.auto_mode.enabled && !config.enable_bash {
            warnings.push(
                "auto_mode is enabled but enable_bash is false — \
                 auto mode may have limited functionality without bash"
                    .to_string(),
            );
        }

        // 5. Model variant validation
        for (name, variant) in &config.model_variants {
            if variant.model.trim().is_empty() {
                warnings.push(format!("model_variants[\"{name}\"].model is empty"));
            }
            if variant.provider.trim().is_empty() {
                warnings.push(format!("model_variants[\"{name}\"].provider is empty"));
            }
            if !(0.0..=2.0).contains(&variant.temperature) {
                warnings.push(format!(
                    "model_variants[\"{name}\"].temperature is {} (expected 0.0–2.0)",
                    variant.temperature
                ));
            }
        }

        // 6. Context tokens sanity check
        if config.max_context_tokens < 1000 {
            warnings.push(format!(
                "max_context_tokens is {} — unusually low, may cause frequent compaction",
                config.max_context_tokens
            ));
        }

        // 7. Bash timeout sanity
        if config.bash_timeout == 0 {
            warnings.push("bash_timeout is 0 — commands will time out immediately".to_string());
        }

        if !warnings.is_empty() {
            for w in &warnings {
                warn!("Config: {}", w);
            }
        }

        warnings
    }

    /// Apply environment variable overrides.
    fn apply_env_overrides(config: &mut AppConfig) {
        Self::apply_env_overrides_with(config, |key| std::env::var(key).ok());
    }

    /// Apply overrides from a variable lookup function.
    ///
    /// Factored out so tests can supply a mock lookup without touching global env.
    fn apply_env_overrides_with(config: &mut AppConfig, get: impl Fn(&str) -> Option<String>) {
        if let Some(provider) = get("OPENDEV_MODEL_PROVIDER") {
            config.model_provider = provider;
        }
        if let Some(model) = get("OPENDEV_MODEL") {
            config.model = model;
        }
        if let Some(val) = get("OPENDEV_MAX_TOKENS")
            && let Ok(max_tokens) = val.parse()
        {
            config.max_tokens = max_tokens;
        }
        if let Some(val) = get("OPENDEV_TEMPERATURE")
            && let Ok(temp) = val.parse()
        {
            config.temperature = temp;
        }
        if let Some(val) = get("OPENDEV_VERBOSE") {
            config.verbose = val == "1" || val.eq_ignore_ascii_case("true");
        }
        if let Some(val) = get("OPENDEV_DEBUG") {
            config.debug_logging = val == "1" || val.eq_ignore_ascii_case("true");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_defaults() {
        let tmp = TempDir::new().unwrap();
        let global = tmp.path().join("global.json");
        let project = tmp.path().join("project.json");

        let config = ConfigLoader::load(&global, &project).unwrap();
        assert_eq!(config.model_provider, "fireworks");
        assert_eq!(config.temperature, 0.6);
    }

    #[test]
    fn test_load_with_global_settings() {
        let tmp = TempDir::new().unwrap();
        let global = tmp.path().join("global.json");
        let project = tmp.path().join("project.json");

        std::fs::write(&global, r#"{"model_provider": "openai", "model": "gpt-4"}"#).unwrap();

        let config = ConfigLoader::load(&global, &project).unwrap();
        assert_eq!(config.model_provider, "openai");
        assert_eq!(config.model, "gpt-4");
        // Defaults preserved for unset fields
        assert_eq!(config.temperature, 0.6);
    }

    #[test]
    fn test_project_overrides_global() {
        let tmp = TempDir::new().unwrap();
        let global = tmp.path().join("global.json");
        let project = tmp.path().join("project.json");

        std::fs::write(&global, r#"{"model_provider": "openai", "model": "gpt-4"}"#).unwrap();
        std::fs::write(&project, r#"{"model": "gpt-4-turbo"}"#).unwrap();

        let config = ConfigLoader::load(&global, &project).unwrap();
        assert_eq!(config.model_provider, "openai"); // from global
        assert_eq!(config.model, "gpt-4-turbo"); // overridden by project
    }

    #[test]
    fn test_merge_preserves_defaults() {
        let base = AppConfig::default();
        let overrides = serde_json::json!({"verbose": true});
        let merged = ConfigLoader::merge(base, overrides);
        assert!(merged.verbose);
        assert_eq!(merged.temperature, 0.6); // default preserved
    }

    #[test]
    fn test_save_and_reload() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");

        let mut config = AppConfig::default();
        config.model_provider = "anthropic".to_string();
        config.model = "claude-3-opus".to_string();
        config.verbose = true;

        ConfigLoader::save(&config, &path).unwrap();

        // Reload
        let loaded = ConfigLoader::load(&path, &tmp.path().join("nonexistent.json")).unwrap();
        assert_eq!(loaded.model_provider, "anthropic");
        assert_eq!(loaded.model, "claude-3-opus");
        assert!(loaded.verbose);
        assert_eq!(loaded.temperature, 0.6); // default preserved
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("dir").join("settings.json");

        let config = AppConfig::default();
        ConfigLoader::save(&config, &path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_save_atomic_no_corruption() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");

        // Save twice; second write should not leave tmp file
        let config = AppConfig::default();
        ConfigLoader::save(&config, &path).unwrap();
        ConfigLoader::save(&config, &path).unwrap();

        assert!(path.exists());
        assert!(!tmp.path().join("settings.json.tmp").exists());
    }

    #[test]
    fn test_validate_default_config() {
        let config = AppConfig::default();
        assert!(ConfigLoader::validate(&config).is_ok());
    }

    #[test]
    fn test_validate_empty_model() {
        let mut config = AppConfig::default();
        config.model = String::new();
        let err = ConfigLoader::validate(&config).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("model name must be a non-empty string"));
    }

    #[test]
    fn test_validate_empty_provider() {
        let mut config = AppConfig::default();
        config.model_provider = "  ".to_string();
        let err = ConfigLoader::validate(&config).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("model_provider must be a non-empty string"));
    }

    #[test]
    fn test_validate_empty_api_key() {
        let mut config = AppConfig::default();
        config.api_key = Some(String::new());
        let err = ConfigLoader::validate(&config).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("api_key must be non-empty when set"));
    }

    #[test]
    fn test_validate_temperature_out_of_range() {
        let mut config = AppConfig::default();
        config.temperature = 2.5;
        let err = ConfigLoader::validate(&config).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("temperature must be between 0.0 and 2.0"));

        // Negative temperature
        config.temperature = -0.1;
        let err = ConfigLoader::validate(&config).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("temperature must be between 0.0 and 2.0"));
    }

    #[test]
    fn test_validate_zero_max_tokens() {
        let mut config = AppConfig::default();
        config.max_tokens = 0;
        let err = ConfigLoader::validate(&config).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("max_tokens must be positive"));
    }

    #[test]
    fn test_validate_multiple_errors() {
        let mut config = AppConfig::default();
        config.model = String::new();
        config.temperature = 3.0;
        config.max_tokens = 0;
        let err = ConfigLoader::validate(&config).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("model name"));
        assert!(msg.contains("temperature"));
        assert!(msg.contains("max_tokens"));
    }

    #[test]
    fn test_validate_boundary_temperature() {
        let mut config = AppConfig::default();
        config.temperature = 0.0;
        assert!(ConfigLoader::validate(&config).is_ok());
        config.temperature = 2.0;
        assert!(ConfigLoader::validate(&config).is_ok());
    }

    // --- Environment variable override tests ---
    // --- Environment variable override tests ---
    // Uses apply_env_overrides_with() with a mock lookup to avoid global env var races.

    fn mock_env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: std::collections::HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn test_env_override_model_provider() {
        let mut config = AppConfig::default();
        assert_eq!(config.model_provider, "fireworks");

        ConfigLoader::apply_env_overrides_with(
            &mut config,
            mock_env(&[("OPENDEV_MODEL_PROVIDER", "anthropic")]),
        );
        assert_eq!(config.model_provider, "anthropic");
    }

    #[test]
    fn test_env_override_model() {
        let mut config = AppConfig::default();
        ConfigLoader::apply_env_overrides_with(
            &mut config,
            mock_env(&[("OPENDEV_MODEL", "gpt-4o")]),
        );
        assert_eq!(config.model, "gpt-4o");
    }

    #[test]
    fn test_env_override_max_tokens() {
        let mut config = AppConfig::default();
        ConfigLoader::apply_env_overrides_with(
            &mut config,
            mock_env(&[("OPENDEV_MAX_TOKENS", "8192")]),
        );
        assert_eq!(config.max_tokens, 8192);
    }

    #[test]
    fn test_env_override_max_tokens_invalid_ignored() {
        let mut config = AppConfig::default();
        config.max_tokens = 99999;
        ConfigLoader::apply_env_overrides_with(
            &mut config,
            mock_env(&[("OPENDEV_MAX_TOKENS", "not_a_number")]),
        );
        assert_eq!(config.max_tokens, 99999);
    }

    #[test]
    fn test_env_override_temperature() {
        let mut config = AppConfig::default();
        ConfigLoader::apply_env_overrides_with(
            &mut config,
            mock_env(&[("OPENDEV_TEMPERATURE", "0.9")]),
        );
        assert!((config.temperature - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_env_override_temperature_invalid_ignored() {
        let mut config = AppConfig::default();
        config.temperature = 1.234;
        ConfigLoader::apply_env_overrides_with(
            &mut config,
            mock_env(&[("OPENDEV_TEMPERATURE", "hot")]),
        );
        assert!((config.temperature - 1.234).abs() < f64::EPSILON);
    }

    #[test]
    fn test_env_override_verbose() {
        let mut config = AppConfig::default();
        assert!(!config.verbose);

        ConfigLoader::apply_env_overrides_with(
            &mut config,
            mock_env(&[("OPENDEV_VERBOSE", "true")]),
        );
        assert!(config.verbose);

        config.verbose = false;
        ConfigLoader::apply_env_overrides_with(&mut config, mock_env(&[("OPENDEV_VERBOSE", "1")]));
        assert!(config.verbose);

        config.verbose = true;
        ConfigLoader::apply_env_overrides_with(
            &mut config,
            mock_env(&[("OPENDEV_VERBOSE", "false")]),
        );
        assert!(!config.verbose);
    }

    #[test]
    fn test_env_override_debug() {
        let mut config = AppConfig::default();
        assert!(!config.debug_logging);

        ConfigLoader::apply_env_overrides_with(&mut config, mock_env(&[("OPENDEV_DEBUG", "TRUE")]));
        assert!(config.debug_logging);
    }

    #[test]
    fn test_env_override_missing_vars_noop() {
        let mut config = AppConfig::default();
        let original = config.clone();
        // Empty map = no env vars set
        ConfigLoader::apply_env_overrides_with(&mut config, mock_env(&[]));
        assert_eq!(config.model_provider, original.model_provider);
        assert_eq!(config.model, original.model);
        assert_eq!(config.max_tokens, original.max_tokens);
        assert!((config.temperature - original.temperature).abs() < f64::EPSILON);
    }

    // --- Unknown field detection tests ---

    #[test]
    fn test_warn_unknown_fields_no_unknowns() {
        let json = serde_json::json!({"model": "gpt-4", "temperature": 0.7});
        let unknown = ConfigLoader::warn_unknown_fields(&json, "test");
        assert!(unknown.is_empty());
    }

    #[test]
    fn test_warn_unknown_fields_detects_typos() {
        let json = serde_json::json!({
            "modle": "gpt-4",
            "temperatre": 0.7,
            "model_provider": "openai"
        });
        let unknown = ConfigLoader::warn_unknown_fields(&json, "test");
        assert_eq!(unknown.len(), 2);
        assert!(unknown.contains(&"modle".to_string()));
        assert!(unknown.contains(&"temperatre".to_string()));
    }

    #[test]
    fn test_warn_unknown_fields_completely_unknown() {
        let json = serde_json::json!({"xyzzy_bogus_field": true});
        let unknown = ConfigLoader::warn_unknown_fields(&json, "test");
        assert_eq!(unknown.len(), 1);
        assert_eq!(unknown[0], "xyzzy_bogus_field");
    }

    #[test]
    fn test_warn_unknown_fields_non_object() {
        // Non-object JSON should return empty (no keys to check)
        let json = serde_json::json!("not an object");
        let unknown = ConfigLoader::warn_unknown_fields(&json, "test");
        assert!(unknown.is_empty());
    }

    #[test]
    fn test_closest_field_exact() {
        let known = ConfigLoader::known_field_names();
        // "model" should match itself exactly (distance 0)
        let closest = ConfigLoader::closest_field("model", &known);
        assert_eq!(closest, Some("model"));
    }

    #[test]
    fn test_closest_field_typo() {
        let known = ConfigLoader::known_field_names();
        // "modle" → "model" (distance 1: transposition)
        let closest = ConfigLoader::closest_field("modle", &known);
        assert_eq!(closest, Some("model"));
    }

    #[test]
    fn test_closest_field_no_match() {
        let known = ConfigLoader::known_field_names();
        // Completely unrelated string should return None
        let closest = ConfigLoader::closest_field("xyzzy_bogus", &known);
        assert!(closest.is_none());
    }

    #[test]
    fn test_edit_distance() {
        assert_eq!(ConfigLoader::edit_distance("model", "model"), 0);
        assert_eq!(ConfigLoader::edit_distance("modle", "model"), 2);
        assert_eq!(ConfigLoader::edit_distance("", "abc"), 3);
        assert_eq!(ConfigLoader::edit_distance("abc", ""), 3);
        assert_eq!(ConfigLoader::edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn test_known_field_names_complete() {
        // Verify that all fields from a serialized default config are in the known set
        let config = AppConfig::default();
        let json = serde_json::to_value(&config).unwrap();
        let known = ConfigLoader::known_field_names();
        if let Some(obj) = json.as_object() {
            for key in obj.keys() {
                assert!(
                    known.contains(key.as_str()),
                    "Field '{}' from AppConfig is missing from known_field_names()",
                    key
                );
            }
        }
    }

    #[test]
    fn test_load_with_unknown_fields_still_works() {
        // Unknown fields should produce warnings but not prevent loading
        let tmp = TempDir::new().unwrap();
        let global = tmp.path().join("global.json");
        let project = tmp.path().join("project.json");

        std::fs::write(
            &global,
            r#"{"model_provider": "openai", "modle": "gpt-4", "unknown_thing": true}"#,
        )
        .unwrap();

        let config = ConfigLoader::load(&global, &project).unwrap();
        assert_eq!(config.model_provider, "openai");
        // "modle" was ignored, default model used
        assert_ne!(config.model, "gpt-4");
    }

    // --- Cross-field validation tests ---

    #[test]
    fn test_cross_field_default_config_no_warnings() {
        let config = AppConfig::default();
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            warnings.is_empty(),
            "Default config should have no warnings, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_model_without_provider() {
        let mut config = AppConfig::default();
        config.model_thinking = Some("gpt-4o-thinking".to_string());
        // model_thinking_provider left as None
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("model_thinking") && w.contains("model_thinking_provider")),
            "Should warn about missing thinking provider: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_model_with_provider_no_warning() {
        let mut config = AppConfig::default();
        config.model_thinking = Some("gpt-4o".to_string());
        config.model_thinking_provider = Some("openai".to_string());
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            !warnings.iter().any(|w| w.contains("model_thinking")),
            "Should not warn when provider is set: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_playbook_weights_sum() {
        let mut config = AppConfig::default();
        config.playbook.scoring_weights.effectiveness = 0.9;
        config.playbook.scoring_weights.recency = 0.9;
        config.playbook.scoring_weights.semantic = 0.9;
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            warnings.iter().any(|w| w.contains("scoring weights sum")),
            "Should warn about weights summing to 2.7: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_playbook_weights_near_one_ok() {
        let mut config = AppConfig::default();
        // Default weights sum to exactly 1.0
        config.playbook.scoring_weights.effectiveness = 0.5;
        config.playbook.scoring_weights.recency = 0.3;
        config.playbook.scoring_weights.semantic = 0.2;
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            !warnings.iter().any(|w| w.contains("scoring weights")),
            "Weights summing to 1.0 should not warn: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_zero_explore_agents() {
        let mut config = AppConfig::default();
        config.plan_mode_explore_agent_count = 0;
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("plan_mode_explore_agent_count")),
            "Should warn about 0 explore agents: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_auto_mode_no_bash() {
        let mut config = AppConfig::default();
        config.auto_mode.enabled = true;
        config.enable_bash = false;
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("auto_mode") && w.contains("enable_bash")),
            "Should warn about auto mode without bash: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_model_variant_empty_model() {
        let mut config = AppConfig::default();
        config.model_variants.insert(
            "bad".to_string(),
            opendev_models::ModelVariant {
                name: "bad".to_string(),
                model: "".to_string(),
                provider: "openai".to_string(),
                temperature: 0.6,
                max_tokens: 4096,
                description: String::new(),
            },
        );
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("model_variants") && w.contains("model is empty")),
            "Should warn about empty model in variant: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_model_variant_bad_temperature() {
        let mut config = AppConfig::default();
        config.model_variants.insert(
            "hot".to_string(),
            opendev_models::ModelVariant {
                name: "hot".to_string(),
                model: "gpt-4".to_string(),
                provider: "openai".to_string(),
                temperature: 5.0,
                max_tokens: 4096,
                description: String::new(),
            },
        );
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("temperature") && w.contains("hot")),
            "Should warn about bad variant temperature: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_low_context_tokens() {
        let mut config = AppConfig::default();
        config.max_context_tokens = 500;
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            warnings.iter().any(|w| w.contains("max_context_tokens")),
            "Should warn about low context tokens: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_zero_bash_timeout() {
        let mut config = AppConfig::default();
        config.bash_timeout = 0;
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            warnings.iter().any(|w| w.contains("bash_timeout")),
            "Should warn about zero bash timeout: {:?}",
            warnings
        );
    }

    #[test]
    fn test_cross_field_multiple_warnings() {
        let mut config = AppConfig::default();
        config.model_thinking = Some("test".to_string());
        config.bash_timeout = 0;
        config.max_context_tokens = 100;
        let warnings = ConfigLoader::validate_cross_field(&config);
        assert!(
            warnings.len() >= 3,
            "Should have multiple warnings: {:?}",
            warnings
        );
    }

    // --- Template substitution tests ---

    #[test]
    fn test_substitute_env_variable() {
        unsafe { std::env::set_var("OPENDEV_TEST_SUB_VAR", "test-value-123") };
        let input = r#"{"api_key": "{env:OPENDEV_TEST_SUB_VAR}"}"#;
        let result = ConfigLoader::substitute_templates(input, Path::new("."));
        assert_eq!(result, r#"{"api_key": "test-value-123"}"#);
        unsafe { std::env::remove_var("OPENDEV_TEST_SUB_VAR") };
    }

    #[test]
    fn test_substitute_env_missing_variable() {
        let input = r#"{"key": "{env:OPENDEV_NONEXISTENT_9999}"}"#;
        let result = ConfigLoader::substitute_templates(input, Path::new("."));
        assert_eq!(result, r#"{"key": ""}"#);
    }

    #[test]
    fn test_substitute_file_inclusion() {
        let tmp = TempDir::new().unwrap();
        let secret_file = tmp.path().join("secret.txt");
        std::fs::write(&secret_file, "my-secret-key\n").unwrap();

        let input = format!(r#"{{"api_key": "{{file:{}}}"}}"#, secret_file.display());
        let result = ConfigLoader::substitute_templates(&input, Path::new("."));
        assert_eq!(result, r#"{"api_key": "my-secret-key"}"#);
    }

    #[test]
    fn test_substitute_file_relative_path() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("key.txt"), "relative-key").unwrap();

        let input = r#"{"api_key": "{file:key.txt}"}"#;
        let result = ConfigLoader::substitute_templates(input, tmp.path());
        assert_eq!(result, r#"{"api_key": "relative-key"}"#);
    }

    #[test]
    fn test_substitute_file_missing() {
        let input = r#"{"api_key": "{file:/nonexistent/path/to/file.txt}"}"#;
        let result = ConfigLoader::substitute_templates(input, Path::new("."));
        assert_eq!(result, r#"{"api_key": ""}"#);
    }

    #[test]
    fn test_substitute_multiple_tokens() {
        unsafe { std::env::set_var("OPENDEV_TEST_MULTI_A", "val-a") };
        unsafe { std::env::set_var("OPENDEV_TEST_MULTI_B", "val-b") };
        let input = r#"{"a": "{env:OPENDEV_TEST_MULTI_A}", "b": "{env:OPENDEV_TEST_MULTI_B}"}"#;
        let result = ConfigLoader::substitute_templates(input, Path::new("."));
        assert_eq!(result, r#"{"a": "val-a", "b": "val-b"}"#);
        unsafe { std::env::remove_var("OPENDEV_TEST_MULTI_A") };
        unsafe { std::env::remove_var("OPENDEV_TEST_MULTI_B") };
    }

    #[test]
    fn test_substitute_no_tokens() {
        let input = r#"{"model": "gpt-4", "temperature": 0.7}"#;
        let result = ConfigLoader::substitute_templates(input, Path::new("."));
        assert_eq!(result, input);
    }

    #[test]
    fn test_substitute_file_with_special_chars() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("special.txt");
        std::fs::write(&file, "value with \"quotes\" and\nnewlines").unwrap();

        let input = format!(r#"{{"data": "{{file:{}}}"}}"#, file.display());
        let result = ConfigLoader::substitute_templates(&input, Path::new("."));
        // Should be JSON-escaped
        assert!(result.contains(r#"value with \"quotes\" and\nnewlines"#));
    }

    #[test]
    fn test_substitute_env_then_load() {
        let tmp = TempDir::new().unwrap();
        unsafe { std::env::set_var("OPENDEV_TEST_PROVIDER_SUB", "anthropic") };

        let config_file = tmp.path().join("config.json");
        std::fs::write(
            &config_file,
            r#"{"model_provider": "{env:OPENDEV_TEST_PROVIDER_SUB}"}"#,
        )
        .unwrap();

        let empty = tmp.path().join("empty.json");
        let config = ConfigLoader::load(&config_file, &empty).unwrap();
        assert_eq!(config.model_provider, "anthropic");

        unsafe { std::env::remove_var("OPENDEV_TEST_PROVIDER_SUB") };
    }
}
