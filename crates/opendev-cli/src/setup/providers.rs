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
#[path = "providers_tests.rs"]
mod tests;
