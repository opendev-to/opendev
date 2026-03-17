//! Formatter configuration models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Formatter configuration.
///
/// Controls auto-formatting of files after edit/write operations.
/// Can disable all formatting, disable specific built-in formatters,
/// or add custom formatters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FormatterConfig {
    /// Set to `false` to disable all formatters.
    Disabled(bool),
    /// Custom configuration with overrides.
    Custom(FormatterOverrides),
}

impl Default for FormatterConfig {
    fn default() -> Self {
        FormatterConfig::Custom(FormatterOverrides::default())
    }
}

impl FormatterConfig {
    /// Check if this is the default (no overrides).
    pub fn is_default(&self) -> bool {
        matches!(self, FormatterConfig::Custom(o) if o.overrides.is_empty())
    }

    /// Check if formatting is globally disabled.
    pub fn is_disabled(&self) -> bool {
        matches!(self, FormatterConfig::Disabled(false))
    }

    /// Get custom formatter overrides (empty if disabled).
    pub fn overrides(&self) -> &HashMap<String, FormatterOverride> {
        match self {
            FormatterConfig::Custom(o) => &o.overrides,
            FormatterConfig::Disabled(_) => {
                static EMPTY: std::sync::LazyLock<HashMap<String, FormatterOverride>> =
                    std::sync::LazyLock::new(HashMap::new);
                &EMPTY
            }
        }
    }
}

/// Map of formatter name to override settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FormatterOverrides {
    #[serde(flatten)]
    pub overrides: HashMap<String, FormatterOverride>,
}

/// Override settings for a single formatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatterOverride {
    /// Disable this formatter.
    #[serde(default)]
    pub disabled: bool,
    /// Custom command (overrides built-in). Use `$FILE` as placeholder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// File extensions this formatter handles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Environment variables to set when running the formatter.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environment: HashMap<String, String>,
}
