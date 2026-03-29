use serde::{Deserialize, Serialize};

/// Sandbox execution configuration.
///
/// Controls microsandbox parameters for the `sandbox_exec` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Whether the sandbox tool is available to agents.
    #[serde(default)]
    pub enabled: bool,

    /// Microsandbox image for the Python runtime.
    #[serde(default = "default_image")]
    pub image: String,

    /// Memory allocation for the sandbox in MB.
    #[serde(default = "default_memory_mb")]
    pub memory_mb: u32,

    /// CPU cores allocated to the sandbox.
    #[serde(default = "default_cpus")]
    pub cpus: f64,

    /// Maximum sandbox loop iterations before aborting.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,

    /// Maximum number of `lm_query()` sub-LLM calls per invocation.
    #[serde(default = "default_max_lm_queries")]
    pub max_lm_queries: u32,

    /// Maximum cost budget in USD per sandbox invocation.
    #[serde(default = "default_cost_budget_usd")]
    pub cost_budget_usd: f64,

    /// Maximum characters in sandbox output before truncation.
    #[serde(default = "default_output_max_chars")]
    pub output_max_chars: usize,

    /// Model to use for `lm_query()` sub-calls. Falls back to session model if `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recursive_model: Option<String>,
}

fn default_image() -> String {
    "microsandbox/python".to_string()
}
fn default_memory_mb() -> u32 {
    512
}
fn default_cpus() -> f64 {
    1.0
}
fn default_max_iterations() -> u32 {
    25
}
fn default_max_lm_queries() -> u32 {
    50
}
fn default_cost_budget_usd() -> f64 {
    2.0
}
fn default_output_max_chars() -> usize {
    50_000
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            image: default_image(),
            memory_mb: default_memory_mb(),
            cpus: default_cpus(),
            max_iterations: default_max_iterations(),
            max_lm_queries: default_max_lm_queries(),
            cost_budget_usd: default_cost_budget_usd(),
            output_max_chars: default_output_max_chars(),
            recursive_model: None,
        }
    }
}
