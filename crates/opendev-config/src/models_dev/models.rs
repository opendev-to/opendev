//! Core data types: RegistryError, ModelInfo, ProviderInfo.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

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
        models.sort_by_key(|m| std::cmp::Reverse(m.context_length));
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

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
