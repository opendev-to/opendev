//! Config validation and unknown field detection.
//!
//! Provides field validation, cross-field relationship checks,
//! typo detection via edit distance, and unknown field warnings.

use opendev_models::AppConfig;
use tracing::warn;

use super::{ConfigError, ConfigLoader};

impl ConfigLoader {
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
    pub(super) fn known_field_names() -> std::collections::HashSet<&'static str> {
        // These are the serde field names (matching JSON keys).
        // Maintained manually to avoid runtime serialization overhead.
        [
            "model_provider",
            "model",
            "model_vlm",
            "model_vlm_provider",
            "api_key",
            "api_base_url",
            "max_tokens",
            "temperature",
            "reasoning_effort",
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
            "sandbox",
            "config_version",
        ]
        .into_iter()
        .collect()
    }

    /// Find the closest known field name using edit distance.
    ///
    /// Returns `Some(suggestion)` if the distance is <= 3 (likely a typo).
    pub(super) fn closest_field<'a>(
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
    pub(super) fn edit_distance(a: &str, b: &str) -> usize {
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
        let model_pairs: &[(&str, &Option<String>, &str, &Option<String>)] = &[(
            "model_vlm",
            &config.model_vlm,
            "model_vlm_provider",
            &config.model_vlm_provider,
        )];
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
}
