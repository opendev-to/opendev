//! Provider definitions and API key validation.
//!
//! Mirrors `opendev/setup/providers.py`.
//!
//! Uses [`ModelRegistry`] from `opendev-config` for dynamic provider/model
//! lookups instead of hardcoded lists.

use opendev_config::models_dev::ModelRegistry;
use thiserror::Error;

// ── Types ───────────────────────────────────────────────────────────────────

/// Static information about an AI provider.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProviderConfig {
    /// Provider identifier (e.g. `"openai"`).
    pub id: String,
    /// Human-readable provider name.
    pub name: String,
    /// Short description shown in the wizard.
    pub description: String,
    /// Environment variable name for the API key.
    pub env_var: String,
    /// Base URL for the provider's API (used for validation).
    pub api_base_url: String,
    /// API format: `"openai"` (OpenAI-compatible) or `"anthropic"`.
    pub api_format: ApiFormat,
    /// Curated list of `(model_id, name, description)` triples.
    pub models: Vec<(String, String, String)>,
}

/// The wire format used by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiFormat {
    OpenAi,
    Anthropic,
}

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum ValidationError {
    #[error("HTTP request failed: {0}")]
    HttpError(String),
    #[error("provider returned status {status}: {body}")]
    ApiError { status: u16, body: String },
    #[error("unexpected response: {0}")]
    Unexpected(String),
}

// ── Provider registry ───────────────────────────────────────────────────────

/// Helper for provider setup operations.
pub struct ProviderSetup;

impl ProviderSetup {
    /// Return a list of `(id, name, description)` for the wizard menu.
    pub fn provider_choices(registry: &ModelRegistry) -> Vec<(String, String, String)> {
        registry
            .list_providers()
            .iter()
            .map(|p| (p.id.clone(), p.name.clone(), p.description.clone()))
            .collect()
    }

    /// Get full config for a provider by ID.
    pub fn get_provider_config(registry: &ModelRegistry, id: &str) -> Option<ProviderConfig> {
        let provider = registry.get_provider(id)?;
        let api_format = if id == "anthropic" {
            ApiFormat::Anthropic
        } else {
            ApiFormat::OpenAi
        };
        let models = provider
            .list_models(None)
            .iter()
            .map(|m| {
                let mut desc = m.format_pricing();
                desc.push_str(&format!(" • {}k context", m.context_length / 1000));
                if m.recommended {
                    desc = format!("Recommended - {}", desc);
                }
                (m.id.clone(), m.name.clone(), desc)
            })
            .collect();
        Some(ProviderConfig {
            id: provider.id.clone(),
            name: provider.name.clone(),
            description: provider.description.clone(),
            env_var: provider.api_key_env.clone(),
            api_base_url: provider.api_base_url.clone(),
            api_format,
            models,
        })
    }

    /// Return models for a provider as `(id, name, description_with_pricing)`.
    pub fn get_provider_models(
        registry: &ModelRegistry,
        provider_id: &str,
    ) -> Vec<(String, String, String)> {
        registry
            .get_provider(provider_id)
            .map(|p| {
                p.list_models(None)
                    .iter()
                    .map(|m| {
                        let mut desc = m.format_pricing();
                        desc.push_str(&format!(" • {}k context", m.context_length / 1000));
                        if m.recommended {
                            desc = format!("Recommended - {}", desc);
                        }
                        (m.id.clone(), m.name.clone(), desc)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Validate an API key by making a lightweight test request.
    pub async fn validate_api_key(
        provider: &ProviderConfig,
        api_key: &str,
    ) -> Result<(), ValidationError> {
        match provider.api_format {
            ApiFormat::OpenAi => validate_openai_key(&provider.api_base_url, api_key).await,
            ApiFormat::Anthropic => validate_anthropic_key(&provider.api_base_url, api_key).await,
        }
    }
}

// ── Validation helpers ──────────────────────────────────────────────────────

async fn validate_openai_key(base_url: &str, api_key: &str) -> Result<(), ValidationError> {
    let effective_base = if base_url.is_empty() {
        "https://api.openai.com/v1"
    } else {
        base_url
    };
    let url = format!("{}/models", effective_base.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| ValidationError::HttpError(e.to_string()))?;

    let status = resp.status().as_u16();
    if status == 200 {
        return Ok(());
    }

    let body = resp.text().await.unwrap_or_default();
    Err(ValidationError::ApiError { status, body })
}

async fn validate_anthropic_key(base_url: &str, api_key: &str) -> Result<(), ValidationError> {
    let effective_base = if base_url.is_empty() {
        "https://api.anthropic.com/v1"
    } else {
        base_url
    };
    let url = format!("{}/messages", effective_base.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .body(r#"{"model":"claude-3-5-haiku-20241022","max_tokens":1,"messages":[{"role":"user","content":"hi"}]}"#)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ValidationError::HttpError(e.to_string()))?;

    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(());
    }
    if status == 401 {
        let body = resp.text().await.unwrap_or_default();
        return Err(ValidationError::ApiError { status, body });
    }
    // 400 (bad request) or 429 (rate limited) means key is valid
    if status == 400 || status == 429 {
        return Ok(());
    }

    let body = resp.text().await.unwrap_or_default();
    Err(ValidationError::ApiError { status, body })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use opendev_config::models_dev::{ModelInfo, ModelRegistry, ProviderInfo};
    use std::collections::HashMap;

    fn test_registry() -> ModelRegistry {
        let mut providers = HashMap::new();

        let mut openai_models = HashMap::new();
        openai_models.insert(
            "gpt-4o".to_string(),
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                provider: "OpenAI".to_string(),
                context_length: 128_000,
                capabilities: vec!["text".to_string(), "vision".to_string()],
                pricing_input: 2.5,
                pricing_output: 10.0,
                pricing_unit: "per million tokens".to_string(),
                serverless: false,
                tunable: false,
                recommended: true,
                max_tokens: Some(16384),
                supports_temperature: true,
                api_type: "chat".to_string(),
            },
        );

        providers.insert(
            "openai".to_string(),
            ProviderInfo {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                description: "GPT-4o, o1, o3 and more".to_string(),
                api_key_env: "OPENAI_API_KEY".to_string(),
                api_base_url: "https://api.openai.com/v1".to_string(),
                models: openai_models,
            },
        );

        let mut anthropic_models = HashMap::new();
        anthropic_models.insert(
            "claude-sonnet-4".to_string(),
            ModelInfo {
                id: "claude-sonnet-4-20250514".to_string(),
                name: "Claude Sonnet 4".to_string(),
                provider: "Anthropic".to_string(),
                context_length: 200_000,
                capabilities: vec!["text".to_string(), "reasoning".to_string()],
                pricing_input: 3.0,
                pricing_output: 15.0,
                pricing_unit: "per million tokens".to_string(),
                serverless: false,
                tunable: false,
                recommended: true,
                max_tokens: Some(8192),
                supports_temperature: true,
                api_type: "chat".to_string(),
            },
        );

        providers.insert(
            "anthropic".to_string(),
            ProviderInfo {
                id: "anthropic".to_string(),
                name: "Anthropic".to_string(),
                description: "Claude 3.5, Claude 4".to_string(),
                api_key_env: "ANTHROPIC_API_KEY".to_string(),
                api_base_url: "https://api.anthropic.com/v1".to_string(),
                models: anthropic_models,
            },
        );

        ModelRegistry { providers }
    }

    #[test]
    fn test_provider_choices() {
        let registry = test_registry();
        let choices = ProviderSetup::provider_choices(&registry);
        assert!(choices.len() >= 2);
        assert!(choices.iter().any(|(id, _, _)| id == "openai"));
        assert!(choices.iter().any(|(id, _, _)| id == "anthropic"));
    }

    #[test]
    fn test_get_provider_config_found() {
        let registry = test_registry();
        let config = ProviderSetup::get_provider_config(&registry, "openai");
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.name, "OpenAI");
        assert_eq!(config.env_var, "OPENAI_API_KEY");
        assert_eq!(config.api_format, ApiFormat::OpenAi);
        assert!(!config.models.is_empty());
    }

    #[test]
    fn test_get_provider_config_not_found() {
        let registry = test_registry();
        let config = ProviderSetup::get_provider_config(&registry, "nonexistent");
        assert!(config.is_none());
    }

    #[test]
    fn test_get_provider_config_anthropic() {
        let registry = test_registry();
        let config = ProviderSetup::get_provider_config(&registry, "anthropic").unwrap();
        assert_eq!(config.api_format, ApiFormat::Anthropic);
        assert!(config.api_base_url.contains("anthropic"));
    }

    #[test]
    fn test_get_provider_models() {
        let registry = test_registry();
        let models = ProviderSetup::get_provider_models(&registry, "openai");
        assert!(!models.is_empty());
        assert!(models.iter().any(|(id, _, _)| id == "gpt-4o"));
    }

    #[test]
    fn test_get_provider_models_nonexistent() {
        let registry = test_registry();
        let models = ProviderSetup::get_provider_models(&registry, "nonexistent");
        assert!(models.is_empty());
    }

    #[test]
    fn test_validation_error_display() {
        let e = ValidationError::HttpError("timeout".into());
        assert!(e.to_string().contains("timeout"));

        let e = ValidationError::ApiError {
            status: 401,
            body: "invalid key".into(),
        };
        assert!(e.to_string().contains("401"));

        let e = ValidationError::Unexpected("weird".into());
        assert!(e.to_string().contains("weird"));
    }
}
