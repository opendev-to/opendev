//! Main agent composing HTTP, LLM, and ReAct loop.
//!
//! Mirrors `opendev/core/agents/main_agent/agent.py`.
//! Uses composition instead of Python's mixin-based inheritance.

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, info};

use opendev_http::adapted_client::AdaptedClient;
use opendev_tools_core::{ToolContext, ToolRegistry};

use crate::llm_calls::{LlmCallConfig, LlmCaller};
use crate::prompts::PromptComposer;
use crate::react_loop::{ReactLoop, ReactLoopConfig};
use crate::response::ResponseCleaner;
use crate::traits::{
    AgentDeps, AgentError, AgentEventCallback, AgentResult, BaseAgent, LlmResponse, TaskMonitor,
};

/// Simple glob matching for tool name patterns.
///
/// Supports `*` (matches zero or more characters) and `?` (matches exactly one character).
/// This is intentionally simple — no `**` or character classes needed for tool names.
fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    glob_match_inner(&p, &n, 0, 0)
}

fn glob_match_inner(pattern: &[char], name: &[char], pi: usize, ni: usize) -> bool {
    let mut pi = pi;
    let mut ni = ni;

    while pi < pattern.len() {
        match pattern[pi] {
            '*' => {
                // Skip consecutive *
                while pi < pattern.len() && pattern[pi] == '*' {
                    pi += 1;
                }
                // * at end matches everything
                if pi == pattern.len() {
                    return true;
                }
                // Try matching * against 0..n characters
                while ni <= name.len() {
                    if glob_match_inner(pattern, name, pi, ni) {
                        return true;
                    }
                    ni += 1;
                }
                return false;
            }
            '?' => {
                if ni >= name.len() {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
            c => {
                if ni >= name.len() || name[ni] != c {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
        }
    }

    ni == name.len()
}

/// Configuration for the MainAgent.
#[derive(Debug, Clone)]
pub struct MainAgentConfig {
    /// Primary model identifier.
    pub model: String,
    /// Optional thinking model identifier.
    pub model_thinking: Option<String>,
    /// Sampling temperature.
    pub temperature: Option<f64>,
    /// Maximum tokens to generate.
    pub max_tokens: Option<u64>,
    /// Working directory for file operations.
    pub working_dir: Option<String>,
    /// Optional list of allowed tool names (for subagent restriction).
    pub allowed_tools: Option<Vec<String>>,
    /// Model provider (e.g., "openai", "anthropic", "gemini", "mistral").
    /// Used for provider-specific schema adaptation.
    pub model_provider: Option<String>,
}

impl MainAgentConfig {
    /// Create a new config with the given model.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            model_thinking: None,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            working_dir: None,
            allowed_tools: None,
            model_provider: None,
        }
    }
}

/// The main agent that coordinates LLM interactions via HTTP.
///
/// Uses composition, not inheritance:
/// - `LlmCaller` handles LLM API call construction and parsing
/// - `ReactLoop` manages the reason-act loop
/// - `PromptComposer` assembles system prompts
/// - `ToolRegistry` provides tool dispatch
/// - `ResponseCleaner` sanitizes model output
/// - `HttpClient` sends requests to the LLM API
pub struct MainAgent {
    config: MainAgentConfig,
    tool_registry: Arc<ToolRegistry>,
    llm_caller: LlmCaller,
    react_loop: ReactLoop,
    _response_cleaner: ResponseCleaner,
    system_prompt: String,
    tool_schemas: Vec<Value>,
    /// HTTP client for LLM API calls.
    http_client: Option<Arc<AdaptedClient>>,
    /// Event callback for streaming agent events to the UI.
    event_callback: Option<Arc<dyn AgentEventCallback>>,
    /// Whether this agent is running as a subagent with restricted tools.
    pub is_subagent: bool,
}

impl MainAgent {
    /// Create a new MainAgent with the given configuration and tool registry.
    pub fn new(config: MainAgentConfig, tool_registry: Arc<ToolRegistry>) -> Self {
        let is_subagent = config.allowed_tools.is_some();

        let llm_config = LlmCallConfig {
            model: config.model.clone(),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            reasoning_effort: None,
        };

        let llm_caller = LlmCaller::new(llm_config);

        let tool_schemas = Self::build_schemas(&tool_registry, config.allowed_tools.as_deref());
        let system_prompt = String::new(); // Built lazily via PromptComposer

        let react_loop = ReactLoop::with_defaults();

        Self {
            config,
            tool_registry,
            llm_caller,
            react_loop,
            _response_cleaner: ResponseCleaner::new(),
            system_prompt,
            tool_schemas,
            http_client: None,
            event_callback: None,
            is_subagent,
        }
    }

    /// Set the HTTP client for LLM API calls.
    pub fn with_http_client(mut self, client: Arc<AdaptedClient>) -> Self {
        self.http_client = Some(client);
        self
    }

    /// Set the HTTP client (mutable reference variant).
    pub fn set_http_client(&mut self, client: Arc<AdaptedClient>) {
        self.http_client = Some(client);
    }

    /// Set the event callback for streaming agent events to the UI.
    pub fn set_event_callback(&mut self, callback: Arc<dyn AgentEventCallback>) {
        self.event_callback = Some(callback);
    }

    /// Build tool schemas, optionally filtering to allowed tools only.
    ///
    /// Tool patterns support wildcards: `"read_*"` matches `read_file`, `read_pdf`, etc.
    /// `"mcp__*"` matches all MCP tools. Exact names also work: `"read_file"`.
    ///
    /// Public variant for use by subagent runners.
    pub fn build_schemas_pub(
        registry: &ToolRegistry,
        allowed_tools: Option<&[String]>,
    ) -> Vec<Value> {
        Self::build_schemas(registry, allowed_tools)
    }

    /// Build tool schemas, optionally filtering to allowed tools only.
    fn build_schemas(registry: &ToolRegistry, allowed_tools: Option<&[String]>) -> Vec<Value> {
        let all_schemas = registry.get_schemas();
        match allowed_tools {
            Some(allowed) => all_schemas
                .into_iter()
                .filter(|schema| {
                    let name = schema
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    allowed.iter().any(|pattern| {
                        if pattern.contains('*') || pattern.contains('?') {
                            // Glob-style matching
                            glob_match(pattern, name)
                        } else {
                            pattern == name
                        }
                    })
                })
                .collect(),
            None => all_schemas,
        }
    }

    /// Set the system prompt directly.
    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.system_prompt = prompt.into();
    }

    /// Set the system prompt using a PromptComposer.
    pub fn set_system_prompt_from_composer(
        &mut self,
        composer: &PromptComposer,
        context: &std::collections::HashMap<String, serde_json::Value>,
    ) {
        self.system_prompt = composer.compose(context);
    }

    /// Set custom ReactLoop configuration.
    pub fn set_react_config(&mut self, config: ReactLoopConfig) {
        self.react_loop = ReactLoop::new(config);
    }

    /// Get a reference to the LLM caller.
    pub fn llm_caller(&self) -> &LlmCaller {
        &self.llm_caller
    }

    /// Get a reference to the tool registry.
    pub fn tool_registry(&self) -> &ToolRegistry {
        &self.tool_registry
    }

    /// Get the current system prompt.
    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    /// Get the current tool schemas.
    pub fn tool_schemas(&self) -> &[Value] {
        &self.tool_schemas
    }

    /// Check if messages contain multimodal image content blocks.
    pub fn messages_contain_images(messages: &[Value]) -> bool {
        for msg in messages {
            if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                for block in content {
                    if block.get("type").and_then(|t| t.as_str()) == Some("image") {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Get the HTTP client, returning an error if not configured.
    fn require_http_client(&self) -> Result<&AdaptedClient, AgentError> {
        self.http_client
            .as_ref()
            .map(|c| c.as_ref())
            .ok_or_else(|| {
                AgentError::ConfigError("HTTP client not configured. Call with_http_client() or set_http_client() before running.".into())
            })
    }
}

#[async_trait]
impl BaseAgent for MainAgent {
    fn build_system_prompt(&self) -> String {
        self.system_prompt.clone()
    }

    fn build_tool_schemas(&self) -> Vec<Value> {
        // Apply provider-specific schema adaptation if provider is known
        if let Some(ref provider) = self.config.model_provider {
            opendev_http::adapters::adapt_for_provider(&self.tool_schemas, provider)
        } else {
            self.tool_schemas.clone()
        }
    }

    fn refresh_tools(&mut self) {
        self.tool_schemas =
            Self::build_schemas(&self.tool_registry, self.config.allowed_tools.as_deref());
        debug!(count = self.tool_schemas.len(), "Refreshed tool schemas");
    }

    async fn call_llm(
        &self,
        messages: &[Value],
        task_monitor: Option<&dyn TaskMonitor>,
    ) -> LlmResponse {
        // Build the action payload (with tools)
        let payload = self
            .llm_caller
            .build_action_payload(messages, &self.tool_schemas);

        let http_client = match self.require_http_client() {
            Ok(c) => c,
            Err(e) => return LlmResponse::fail(e.to_string()),
        };

        debug!(model = %self.config.model, "Sending LLM request");

        let http_result = match http_client.post_json(&payload, None).await {
            Ok(r) => r,
            Err(e) => return LlmResponse::fail(e.to_string()),
        };

        if http_result.interrupted {
            // Background request also cancels the token — distinguish from hard interrupt
            if task_monitor.is_some_and(|m| m.is_background_requested()) {
                info!("Background requested during LLM call in MainAgent");
            }
            return LlmResponse::interrupted();
        }

        if !http_result.success {
            return LlmResponse::fail(
                http_result
                    .error
                    .unwrap_or_else(|| "HTTP request failed".to_string()),
            );
        }

        let body = match http_result.body {
            Some(b) => b,
            None => return LlmResponse::fail("Empty response body"),
        };

        let response = self.llm_caller.parse_action_response(&body);

        // Track token usage
        if let Some(monitor) = task_monitor
            && let Some(ref usage) = response.usage
            && let Some(total) = usage.get("total_tokens").and_then(|t| t.as_u64())
        {
            monitor.update_tokens(total);
        }

        response
    }

    async fn run(
        &self,
        message: &str,
        _deps: &AgentDeps,
        message_history: Option<Vec<Value>>,
        task_monitor: Option<&dyn TaskMonitor>,
    ) -> Result<AgentResult, AgentError> {
        let http_client = self.require_http_client()?;

        let mut messages = message_history.unwrap_or_default();

        // Ensure system message is first
        if messages.is_empty() || messages[0].get("role").and_then(|r| r.as_str()) != Some("system")
        {
            messages.insert(
                0,
                serde_json::json!({
                    "role": "system",
                    "content": self.system_prompt
                }),
            );
        }

        // Add user message
        messages.push(serde_json::json!({
            "role": "user",
            "content": message
        }));

        info!(
            model = %self.config.model,
            is_subagent = self.is_subagent,
            message_count = messages.len(),
            "Starting agent run"
        );

        // Build tool context from deps and config
        let cwd_fallback = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());
        let working_dir = self.config.working_dir.as_deref().unwrap_or(&cwd_fallback);
        let tool_context = ToolContext::new(working_dir);

        // Run the full ReAct loop
        self.react_loop
            .run(
                &self.llm_caller,
                http_client,
                &mut messages,
                &self.tool_schemas,
                &self.tool_registry,
                &tool_context,
                task_monitor,
                self.event_callback.as_deref(),
                None,
                None,
                None,
                None, // todo_manager: subagents don't track todos
                None, // cancel: subagent cancellation handled by task_monitor
                None, // tool_approval_tx: subagents auto-approve
            )
            .await
    }
}

impl std::fmt::Debug for MainAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainAgent")
            .field("model", &self.config.model)
            .field("is_subagent", &self.is_subagent)
            .field("tool_count", &self.tool_schemas.len())
            .field("has_http_client", &self.http_client.is_some())
            .field("has_event_callback", &self.event_callback.is_some())
            .finish()
    }
}

#[cfg(test)]
#[path = "main_agent_tests.rs"]
mod tests;
