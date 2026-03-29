//! Configuration models.

mod agent;
mod formatter;
mod permissions;

pub use agent::{AgentConfigInline, ModelVariant};
pub use formatter::{FormatterConfig, FormatterOverride, FormatterOverrides};
pub use permissions::{PermissionConfig, ToolPermission};

use permissions::default_true;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Shared default functions used by sub-modules via `super::` ──

pub(crate) fn default_temperature() -> f64 {
    0.6
}
pub(crate) fn default_max_tokens() -> u32 {
    16384
}

// ── AutoModeConfig ──

/// Auto mode configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoModeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_max_operations")]
    pub max_operations: u32,
    #[serde(default = "default_require_confirmation_after")]
    pub require_confirmation_after: u32,
    #[serde(default = "default_true")]
    pub dangerous_operations_require_approval: bool,
}

fn default_max_operations() -> u32 {
    10
}
fn default_require_confirmation_after() -> u32 {
    5
}

impl Default for AutoModeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_operations: 10,
            require_confirmation_after: 5,
            dangerous_operations_require_approval: true,
        }
    }
}

// ── OperationConfig ──

/// Operation-specific settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationConfig {
    #[serde(default = "default_true")]
    pub show_diffs: bool,
    #[serde(default = "default_true")]
    pub backup_before_edit: bool,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
    #[serde(default)]
    pub allowed_extensions: Vec<String>,
}

fn default_max_file_size() -> u64 {
    1_000_000
}

impl Default for OperationConfig {
    fn default() -> Self {
        Self {
            show_diffs: true,
            backup_before_edit: true,
            max_file_size: 1_000_000,
            allowed_extensions: Vec::new(),
        }
    }
}

// ── PlaybookConfig ──

/// Scoring weights for ACE playbook bullet selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookScoringWeights {
    #[serde(default = "default_effectiveness")]
    pub effectiveness: f64,
    #[serde(default = "default_recency")]
    pub recency: f64,
    #[serde(default = "default_semantic")]
    pub semantic: f64,
}

fn default_effectiveness() -> f64 {
    0.5
}
fn default_recency() -> f64 {
    0.3
}
fn default_semantic() -> f64 {
    0.2
}

impl Default for PlaybookScoringWeights {
    fn default() -> Self {
        Self {
            effectiveness: 0.5,
            recency: 0.3,
            semantic: 0.2,
        }
    }
}

impl PlaybookScoringWeights {
    /// Validate that all weights are between 0.0 and 1.0.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("effectiveness", self.effectiveness),
            ("recency", self.recency),
            ("semantic", self.semantic),
        ] {
            if !(0.0..=1.0).contains(&value) {
                return Err(format!(
                    "{name} weight must be between 0.0 and 1.0, got {value}"
                ));
            }
        }
        Ok(())
    }
}

/// ACE playbook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookConfig {
    #[serde(default = "default_max_strategies")]
    pub max_strategies: u32,
    #[serde(default = "default_true")]
    pub use_selection: bool,
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    #[serde(default = "default_embedding_provider")]
    pub embedding_provider: String,
    #[serde(default)]
    pub scoring_weights: PlaybookScoringWeights,
    #[serde(default = "default_true")]
    pub cache_embeddings: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_file: Option<String>,
}

fn default_max_strategies() -> u32 {
    30
}
fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}
fn default_embedding_provider() -> String {
    "openai".to_string()
}

impl Default for PlaybookConfig {
    fn default() -> Self {
        Self {
            max_strategies: 30,
            use_selection: true,
            embedding_model: "text-embedding-3-small".to_string(),
            embedding_provider: "openai".to_string(),
            scoring_weights: PlaybookScoringWeights::default(),
            cache_embeddings: true,
            cache_file: None,
        }
    }
}

// ── ChannelsConfig ──

/// Channel integrations configuration (Telegram, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelsConfig {
    /// Telegram bot configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram: Option<TelegramChannelConfig>,
}

/// DM access policy for a channel.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DmPolicy {
    Open,
    #[default]
    Pairing,
    Allowlist,
}

/// Telegram channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChannelConfig {
    /// Bot token from @BotFather.
    pub bot_token: String,
    /// Whether the Telegram channel is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Only respond in groups when @mentioned or replied to.
    #[serde(default = "default_true")]
    pub group_mention_only: bool,
    /// DM access policy: "open", "pairing", or "allowlist".
    #[serde(default)]
    pub dm_policy: DmPolicy,
    /// Allowed Telegram user IDs (as strings).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_users: Vec<String>,
}

// ── AppConfig ──

/// Application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // AI Provider settings - Three model system
    #[serde(default = "default_model_provider")]
    pub model_provider: String,
    #[serde(default = "default_model")]
    pub model: String,

    // Vision/Multi-modal model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_vlm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_vlm_provider: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f64,

    // Reasoning effort for models that support extended thinking ("low", "medium", "high", "none")
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,

    // Session settings
    #[serde(default = "default_auto_save_interval")]
    pub auto_save_interval: u32,
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u64,

    // UI settings
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub debug_logging: bool,
    #[serde(default = "default_color_scheme")]
    pub color_scheme: String,
    #[serde(default = "default_true")]
    pub show_token_count: bool,
    #[serde(default = "default_true")]
    pub enable_sound: bool,

    // Permissions
    #[serde(default)]
    pub permissions: PermissionConfig,

    // Operation settings
    #[serde(default = "default_true")]
    pub enable_bash: bool,
    #[serde(default = "default_bash_timeout")]
    pub bash_timeout: u32,
    #[serde(default)]
    pub auto_mode: AutoModeConfig,
    #[serde(default)]
    pub operation: OperationConfig,
    #[serde(default = "default_max_undo_history")]
    pub max_undo_history: u32,

    // Session intelligence
    #[serde(default = "default_true")]
    pub topic_detection: bool,

    // ACE Playbook settings
    #[serde(default)]
    pub playbook: PlaybookConfig,

    // Plan mode configuration
    #[serde(default = "default_plan_mode_workflow")]
    pub plan_mode_workflow: String,
    #[serde(default = "default_plan_mode_explore_agent_count")]
    pub plan_mode_explore_agent_count: u32,
    #[serde(default = "default_plan_mode_plan_agent_count")]
    pub plan_mode_plan_agent_count: u32,
    #[serde(default = "default_plan_mode_explore_variant")]
    pub plan_mode_explore_variant: String,

    // Custom instructions -- file paths, glob patterns, or `~/` paths
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<String>,

    // Additional skill directories -- file paths or `~/` paths
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_paths: Vec<String>,

    // Remote URLs to discover skills from (fetches index.json)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_urls: Vec<String>,

    // Default agent to use for new sessions (e.g. "general", "explore")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,

    // Inline agent definitions/overrides from config.
    // Keys are agent identifiers (e.g. "build", "explore", or custom names).
    // Overrides merge onto builtin agents; new keys create custom agents.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agents: HashMap<String, AgentConfigInline>,

    // Model variants
    #[serde(default)]
    pub model_variants: HashMap<String, ModelVariant>,

    // Formatter configuration (disable built-in or add custom formatters)
    #[serde(default, skip_serializing_if = "FormatterConfig::is_default")]
    pub formatter: FormatterConfig,

    // Channel integrations (Telegram, etc.)
    #[serde(default, skip_serializing_if = "is_channels_default")]
    pub channels: ChannelsConfig,

    // Config version for migration support
    #[serde(default = "default_config_version")]
    pub config_version: u32,
}

fn is_channels_default(c: &ChannelsConfig) -> bool {
    c.telegram.is_none()
}

fn default_config_version() -> u32 {
    1
}
fn default_model_provider() -> String {
    "fireworks".to_string()
}
fn default_model() -> String {
    "accounts/fireworks/models/kimi-k2-instruct-0905".to_string()
}
fn default_auto_save_interval() -> u32 {
    5
}
fn default_max_context_tokens() -> u64 {
    100_000
}
fn default_color_scheme() -> String {
    "monokai".to_string()
}
fn default_bash_timeout() -> u32 {
    30
}
fn default_max_undo_history() -> u32 {
    50
}
fn default_plan_mode_workflow() -> String {
    "5-phase".to_string()
}
fn default_plan_mode_explore_agent_count() -> u32 {
    3
}
fn default_plan_mode_plan_agent_count() -> u32 {
    1
}
fn default_plan_mode_explore_variant() -> String {
    "enabled".to_string()
}
fn default_reasoning_effort() -> String {
    "medium".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model_provider: default_model_provider(),
            model: default_model(),
            model_vlm: None,
            model_vlm_provider: None,
            api_key: None,
            api_base_url: None,
            max_tokens: 16384,
            temperature: 0.6,
            reasoning_effort: "medium".to_string(),
            auto_save_interval: 5,
            max_context_tokens: 100_000,
            verbose: false,
            debug_logging: false,
            color_scheme: "monokai".to_string(),
            show_token_count: true,
            enable_sound: true,
            permissions: PermissionConfig::default(),
            enable_bash: true,
            bash_timeout: 30,
            auto_mode: AutoModeConfig::default(),
            operation: OperationConfig::default(),
            max_undo_history: 50,
            topic_detection: true,
            playbook: PlaybookConfig::default(),
            plan_mode_workflow: "5-phase".to_string(),
            plan_mode_explore_agent_count: 3,
            plan_mode_plan_agent_count: 1,
            plan_mode_explore_variant: "enabled".to_string(),
            instructions: Vec::new(),
            skill_paths: Vec::new(),
            skill_urls: Vec::new(),
            default_agent: None,
            agents: HashMap::new(),
            model_variants: HashMap::new(),
            formatter: FormatterConfig::default(),
            channels: ChannelsConfig::default(),
            config_version: default_config_version(),
        }
    }
}

impl AppConfig {
    /// Resolve the model and provider for a named agent role (e.g. "compact").
    ///
    /// Looks up `self.agents[role]` and falls back to the primary model/provider.
    pub fn resolve_agent_role(&self, role: &str) -> (String, String) {
        if let Some(agent) = self.agents.get(role) {
            let model = agent.model.as_deref().unwrap_or(&self.model);
            let provider = agent.provider.as_deref().unwrap_or(&self.model_provider);
            (model.to_string(), provider.to_string())
        } else {
            (self.model.clone(), self.model_provider.clone())
        }
    }

    /// Get the API key from config or the environment.
    ///
    /// Resolution order:
    /// 1. `registry_env_var` (from models.dev registry, e.g. `ZHIPU_API_KEY`)
    /// 2. Well-known env var for the provider (hardcoded fallback)
    /// 3. Convention-based env var: `{PROVIDER}_API_KEY` (e.g. `zai` → `ZAI_API_KEY`)
    /// 4. `self.api_key` (stored in config by the setup wizard)
    /// 5. `OPENAI_API_KEY` (last resort for truly unknown providers)
    pub fn get_api_key_with_env(&self, registry_env_var: Option<&str>) -> Result<String, String> {
        // Try registry env var first (authoritative for models.dev providers)
        if let Some(env_var) = registry_env_var
            && !env_var.is_empty()
            && let Ok(key) = std::env::var(env_var)
            && !key.is_empty()
        {
            return Ok(key);
        }

        // Try well-known env var for built-in providers
        let builtin_env = Self::builtin_env_var(&self.model_provider);
        if !builtin_env.is_empty()
            && let Ok(key) = std::env::var(builtin_env)
            && !key.is_empty()
        {
            return Ok(key);
        }

        // Convention-based: derive env var from provider ID → {PROVIDER}_API_KEY
        // e.g. "zai" → "ZAI_API_KEY", "siliconflow" → "SILICONFLOW_API_KEY"
        let convention_env = Self::convention_env_var(&self.model_provider);
        if !convention_env.is_empty()
            && convention_env != builtin_env
            && registry_env_var != Some(convention_env.as_str())
            && let Ok(key) = std::env::var(&convention_env)
            && !key.is_empty()
        {
            return Ok(key);
        }

        // Fall back to config-stored API key (from setup wizard)
        if let Some(ref key) = self.api_key {
            return Ok(key.clone());
        }

        // Last resort: try OPENAI_API_KEY for unknown providers
        if builtin_env.is_empty()
            && registry_env_var.is_none_or(str::is_empty)
            && let Ok(key) = std::env::var("OPENAI_API_KEY")
            && !key.is_empty()
        {
            return Ok(key);
        }

        let hint = registry_env_var
            .filter(|s| !s.is_empty())
            .or(Some(builtin_env).filter(|s| !s.is_empty()))
            .or(Some(convention_env.as_str()).filter(|s| !s.is_empty()))
            .unwrap_or("OPENAI_API_KEY");
        Err(format!(
            "No API key found. Set {} environment variable",
            hint
        ))
    }

    /// Convenience wrapper that calls [`get_api_key_with_env`] without registry info.
    pub fn get_api_key(&self) -> Result<String, String> {
        self.get_api_key_with_env(None)
    }

    /// Map well-known provider IDs to their conventional env var names.
    /// Only covers providers that predate the models.dev registry.
    fn builtin_env_var(provider: &str) -> &'static str {
        match provider {
            "fireworks" | "fireworks-ai" => "FIREWORKS_API_KEY",
            "anthropic" => "ANTHROPIC_API_KEY",
            "openai" => "OPENAI_API_KEY",
            "azure" => "AZURE_OPENAI_API_KEY",
            "groq" => "GROQ_API_KEY",
            "mistral" => "MISTRAL_API_KEY",
            "deepinfra" => "DEEPINFRA_API_KEY",
            "openrouter" => "OPENROUTER_API_KEY",
            "deepseek" => "DEEPSEEK_API_KEY",
            "cohere" => "COHERE_API_KEY",
            "togetherai" | "together" => "TOGETHER_API_KEY",
            "perplexity" | "perplexity-agent" => "PERPLEXITY_API_KEY",
            "xai" => "XAI_API_KEY",
            "google" | "gemini" => "GOOGLE_GENERATIVE_AI_API_KEY",
            _ => "",
        }
    }

    /// Derive a convention-based env var from the provider ID.
    ///
    /// Strips common suffixes like `-coding-plan`, `-cn`, `-agent`, then
    /// uppercases and converts hyphens to underscores: `zai` → `ZAI_API_KEY`,
    /// `siliconflow-cn` → `SILICONFLOW_API_KEY`.
    fn convention_env_var(provider: &str) -> String {
        // Strip common suffixes that don't affect the key name
        let base = provider
            .strip_suffix("-coding-plan")
            .or_else(|| provider.strip_suffix("-cn"))
            .or_else(|| provider.strip_suffix("-agent"))
            .unwrap_or(provider);
        if base.is_empty() {
            return String::new();
        }
        format!("{}_API_KEY", base.to_uppercase().replace('-', "_"))
    }
}

#[cfg(test)]
mod tests;
