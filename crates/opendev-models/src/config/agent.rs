//! Agent and model variant configuration models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A named model configuration variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVariant {
    pub name: String,
    pub model: String,
    pub provider: String,
    #[serde(default = "super::default_temperature")]
    pub temperature: f64,
    #[serde(default = "super::default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub description: String,
}

/// Inline agent configuration from opendev.json.
///
/// Allows defining new agents or overriding builtin agents directly
/// in the config file. All fields are optional — only specified fields
/// are applied as overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfigInline {
    /// Model override (e.g. "gpt-4o", "claude-opus-4-5").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// System prompt override or addition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Description of when to use this agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Temperature override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Top-p override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// Max iterations (steps) for the react loop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<usize>,
    /// Agent mode: "primary", "subagent", or "all".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Display color (hex string like "#FF6600").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Hide from autocomplete listings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// Disable/remove this agent entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    /// Per-tool permission rules (tool pattern -> "allow"/"deny"/"ask").
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub permission: HashMap<String, String>,
}
