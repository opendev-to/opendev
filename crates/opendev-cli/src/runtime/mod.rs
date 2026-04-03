//! Agent runtime — central orchestration struct.
//!
//! Owns all services and coordinates the full pipeline:
//! CLI → REPL → QueryEnhancer → ReactLoop → ToolExecutor → display

pub mod background;
mod channel_executor;
mod query;
mod tools;

pub use channel_executor::ChannelAgentExecutor;
// Re-export for backward compat (used in tests)
#[cfg(test)]
pub use tools::build_system_prompt;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use tracing::{debug, info};

use opendev_agents::llm_calls::{LlmCallConfig, LlmCaller};
use opendev_agents::react_loop::{ReactLoop, ReactLoopConfig};
use opendev_context::{ArtifactIndex, ContextCompactor};
use opendev_history::SessionManager;
use opendev_history::topic_detector::TopicDetector;
use opendev_http::HttpClient;
use opendev_http::adapted_client::AdaptedClient;
use opendev_http::adapters::base::ProviderAdapter;
use opendev_mcp::McpManager;
use opendev_models::AppConfig;
use opendev_repl::HandlerRegistry;
use opendev_repl::query_enhancer::QueryEnhancer;
use opendev_runtime::{CostTracker, SessionDebugLogger};
use opendev_tools_core::{BaseTool, ToolRegistry};
use opendev_tools_impl::*;

/// Central orchestrator that owns all agent services.
///
/// Connects: config → session → prompt → LLM → tools → response
#[allow(dead_code)]
pub struct AgentRuntime {
    /// Application configuration.
    pub config: AppConfig,
    /// Working directory.
    pub working_dir: PathBuf,
    /// Session manager for conversation persistence.
    pub session_manager: SessionManager,
    /// Tool registry with all available tools (Arc for sharing with subagents).
    pub tool_registry: Arc<ToolRegistry>,
    /// Handler middleware for pre/post tool processing.
    pub handler_registry: HandlerRegistry,
    /// Query enhancer for @ file injection and message preparation.
    pub query_enhancer: QueryEnhancer,
    /// HTTP client for LLM API calls (Arc for sharing with subagents).
    pub http_client: Arc<AdaptedClient>,
    /// LLM caller configuration.
    pub llm_caller: LlmCaller,
    /// ReAct loop.
    pub react_loop: ReactLoop,
    /// Cost tracker (shared with the react loop for per-call recording).
    pub cost_tracker: Mutex<CostTracker>,
    /// Artifact index tracking file operations (survives compaction).
    pub artifact_index: Mutex<ArtifactIndex>,
    /// Context compactor for auto-compaction when approaching context limits.
    pub compactor: Mutex<ContextCompactor>,
    /// Shared todo manager for TUI panel synchronization.
    pub todo_manager: Arc<Mutex<opendev_runtime::TodoManager>>,
    /// Tool approval sender — passed to react loop for gating bash execution.
    pub tool_approval_tx: Option<opendev_runtime::ToolApprovalSender>,
    /// Channel receivers for TUI bridging (taken once by tui_runner).
    pub channel_receivers: Option<ToolChannelReceivers>,
    /// MCP manager for MCP server connections (shared Arc for bridge tools).
    pub mcp_manager: Option<Arc<McpManager>>,
    /// Shared skill loader for re-registering invoke_skill with MCP support.
    pub(super) skill_loader: Arc<Mutex<opendev_agents::SkillLoader>>,
    /// LLM-based topic detector for auto-generating session titles.
    pub(super) topic_detector: TopicDetector,
    /// Shadow git snapshot manager for tracking file changes per query.
    pub(super) snapshot_manager: Arc<Mutex<opendev_history::SnapshotManager>>,
    /// Per-session debug logger for LLM interactions (noop when debug_logging is off).
    pub debug_logger: Arc<SessionDebugLogger>,
    /// Prompt composer for per-turn system prompt composition with section caching.
    pub prompt_composer: Mutex<opendev_agents::prompts::PromptComposer>,
    /// Prompt context (runtime values for conditional section inclusion).
    pub prompt_context: Mutex<opendev_agents::prompts::PromptContext>,
}

/// Receivers returned from tool registration for TUI bridging.
pub struct ToolChannelReceivers {
    pub ask_user_rx: opendev_runtime::AskUserReceiver,
    pub plan_approval_rx: opendev_runtime::PlanApprovalReceiver,
    pub tool_approval_rx: opendev_runtime::ToolApprovalReceiver,
    pub subagent_event_rx:
        Option<tokio::sync::mpsc::UnboundedReceiver<opendev_tools_impl::SubagentEvent>>,
}

impl AgentRuntime {
    /// Create a new agent runtime with all tools registered.
    pub fn new(
        config: AppConfig,
        working_dir: &Path,
        session_manager: SessionManager,
    ) -> Result<Self, String> {
        // Set up tool registry with overflow storage for truncated tool outputs.
        let overflow_dir = working_dir.join(".opendev").join("tool-output");
        // Clean up overflow files older than 7 days on startup.
        opendev_tools_core::cleanup_overflow_dir(&overflow_dir);
        let tool_registry = Arc::new(ToolRegistry::with_overflow_dir(overflow_dir));
        let (todo_manager, mut channel_receivers, tool_approval_tx) =
            tools::register_default_tools(&tool_registry);

        // Register custom tools from .opendev/tools/ and .opencode/tool/ directories.
        let custom_tools = opendev_tools_impl::custom_tool::discover_custom_tools(working_dir);
        for tool in custom_tools {
            info!(name = tool.name(), "Registered custom tool");
            tool_registry.register(Arc::new(tool));
        }

        // Compute git root once — shared by skill scanning and subagent manager below.
        let git_root = opendev_agents::git_root(working_dir);

        // Register invoke_skill tool with project-local and user-global skill dirs.
        // Scans .opendev/skills at each level from working_dir up to git root,
        // then global dir, then config-specified skill_paths (lowest priority).
        let mut skill_dirs = Vec::new();

        // Walk from working_dir up to git root, scanning for skill directories
        // at each level. This supports monorepos where subdirectories can have
        // their own skill overrides.
        let stop_dir = git_root.as_deref().unwrap_or(working_dir);

        {
            let mut current = working_dir.to_path_buf();
            loop {
                skill_dirs.push(current.join(".opendev").join("skills"));
                if current == stop_dir || !current.pop() {
                    break;
                }
            }
        }

        // Global (home) skill directory
        if let Some(home) = dirs_next::home_dir() {
            skill_dirs.push(home.join(".opendev").join("skills"));
        }
        // Append config-specified skill paths (resolved relative to working_dir, ~/expanded)
        for path in &config.skill_paths {
            let resolved = if let Some(rest) = path.strip_prefix("~/") {
                dirs_next::home_dir()
                    .map(|h| h.join(rest))
                    .unwrap_or_else(|| PathBuf::from(path))
            } else if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                working_dir.join(path)
            };
            skill_dirs.push(resolved);
        }
        let mut skill_loader_inner = opendev_agents::SkillLoader::new(skill_dirs);
        // Add remote skill URLs from config
        if !config.skill_urls.is_empty() {
            skill_loader_inner.add_urls(config.skill_urls.clone());
        }
        let skill_loader = Arc::new(Mutex::new(skill_loader_inner));
        tool_registry.register(Arc::new(InvokeSkillTool::new(Arc::clone(&skill_loader))));
        info!(
            tool_count = tool_registry.tool_names().len(),
            "Registered default tools (before subagent)"
        );

        let handler_registry = HandlerRegistry::new();
        let query_enhancer = QueryEnhancer::new(working_dir.to_path_buf());

        // Load model registry early — needed for provider metadata (api_key_env,
        // api_base_url) and model capabilities (temperature, max_tokens).
        let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
        let registry = opendev_config::ModelRegistry::load_from_cache(&paths.global_cache_dir());

        // Configure HTTP client based on provider.
        // Consult the registry for the correct API key env var and base URL,
        // so all registry-based providers work out of the box.
        let provider_info = registry.get_provider_or_builtin(&config.model_provider);
        let registry_env = provider_info.as_ref().map(|pi| pi.api_key_env.as_str());
        let api_key = config
            .get_api_key_with_env(registry_env)
            .unwrap_or_default();
        let registry_base_url = provider_info
            .as_ref()
            .map(|pi| pi.api_base_url.clone())
            .filter(|s| !s.is_empty());

        // Auto-detect provider from API key if not explicitly set
        let provider = AdaptedClient::resolve_provider(&config.model_provider, &api_key);
        debug!(provider = %provider, "Resolved model provider");

        // Effective base URL: config override > registry > provider default
        let effective_base_url = config.api_base_url.clone().or(registry_base_url);

        let (api_url, headers, adapter): (
            String,
            HeaderMap,
            Option<Box<dyn opendev_http::adapters::base::ProviderAdapter>>,
        ) = match provider.as_str() {
            "anthropic" => {
                let adapter = opendev_http::adapters::anthropic::AnthropicAdapter::new();
                let url = effective_base_url.unwrap_or_else(|| adapter.api_url().to_string());
                let mut hdrs = HeaderMap::new();
                // Anthropic uses x-api-key header (not Bearer)
                if let Ok(val) = HeaderValue::from_str(&api_key) {
                    hdrs.insert("x-api-key", val);
                }
                hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                for (key, value) in adapter.extra_headers() {
                    if let (Ok(k), Ok(v)) = (
                        reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                        HeaderValue::from_str(&value),
                    ) {
                        hdrs.insert(k, v);
                    }
                }
                (
                    url,
                    hdrs,
                    Some(
                        Box::new(adapter) as Box<dyn opendev_http::adapters::base::ProviderAdapter>
                    ),
                )
            }
            "openai" => {
                // OpenAI uses /v1/responses (Responses API) with Bearer auth
                let adapter = opendev_http::adapters::openai::OpenAiAdapter::new();
                let url = effective_base_url.unwrap_or_else(|| adapter.api_url().to_string());
                let mut hdrs = HeaderMap::new();
                if let Ok(val) = HeaderValue::from_str(&format!("Bearer {api_key}")) {
                    hdrs.insert(AUTHORIZATION, val);
                }
                hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                (
                    url,
                    hdrs,
                    Some(
                        Box::new(adapter) as Box<dyn opendev_http::adapters::base::ProviderAdapter>
                    ),
                )
            }
            "ollama" => {
                let adapter = opendev_http::adapters::ollama::OllamaAdapter::new();
                let url = effective_base_url.unwrap_or_else(|| adapter.api_url().to_string());
                let mut hdrs = HeaderMap::new();
                hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                (
                    url,
                    hdrs,
                    Some(
                        Box::new(adapter) as Box<dyn opendev_http::adapters::base::ProviderAdapter>
                    ),
                )
            }
            "gemini" | "google" => {
                let adapter = opendev_http::adapters::gemini::GeminiAdapter::new(&config.model);
                let api_url = effective_base_url
                    .map(|base| {
                        opendev_http::adapters::gemini::gemini_api_url(&base, &config.model)
                    })
                    .unwrap_or_else(|| {
                        opendev_http::adapters::gemini::gemini_api_url(
                            adapter.api_url(),
                            &config.model,
                        )
                    });
                let mut hdrs = HeaderMap::new();
                // Gemini uses x-goog-api-key header
                if let Ok(val) = HeaderValue::from_str(&api_key) {
                    hdrs.insert("x-goog-api-key", val);
                }
                hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                (
                    api_url,
                    hdrs,
                    Some(
                        Box::new(adapter) as Box<dyn opendev_http::adapters::base::ProviderAdapter>
                    ),
                )
            }
            "azure" => {
                let base = effective_base_url
                    .as_deref()
                    .unwrap_or("https://api.openai.com");
                let deployment = &config.model;
                let url = format!(
                    "{}/openai/deployments/{deployment}/chat/completions?api-version=2024-10-21",
                    base.trim_end_matches('/')
                );
                let mut hdrs = HeaderMap::new();
                if let Ok(val) = HeaderValue::from_str(&api_key) {
                    hdrs.insert("api-key", val);
                }
                hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                let adapter = opendev_http::adapters::chat_completions::ChatCompletionsAdapter::new(
                    url.clone(),
                );
                (
                    url,
                    hdrs,
                    Some(
                        Box::new(adapter) as Box<dyn opendev_http::adapters::base::ProviderAdapter>
                    ),
                )
            }
            provider => {
                // OpenAI-compatible providers — use registry/config base URL or fall back
                let url = effective_base_url
                    .map(|base| {
                        let trimmed = base.trim_end_matches('/');
                        if trimmed.ends_with("/chat/completions") {
                            trimmed.to_string()
                        } else {
                            format!("{trimmed}/chat/completions")
                        }
                    })
                    .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string());

                let mut hdrs = HeaderMap::new();
                if let Ok(val) = HeaderValue::from_str(&format!("Bearer {api_key}")) {
                    hdrs.insert(AUTHORIZATION, val);
                }
                hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                if provider == "openrouter"
                    && let Ok(val) = HeaderValue::from_str("https://opendev.ai")
                {
                    hdrs.insert("HTTP-Referer", val);
                }
                let adapter = opendev_http::adapters::chat_completions::ChatCompletionsAdapter::new(
                    url.clone(),
                );
                (
                    url,
                    hdrs,
                    Some(
                        Box::new(adapter) as Box<dyn opendev_http::adapters::base::ProviderAdapter>
                    ),
                )
            }
        };

        let circuit_breaker =
            std::sync::Arc::new(opendev_http::CircuitBreaker::with_defaults(&provider));
        let raw_http_client = HttpClient::new(api_url.clone(), headers, None)
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?
            .with_circuit_breaker(circuit_breaker);

        // DashScope Coding Plan endpoint rejects reqwest — must use curl subprocess.
        let needs_curl = api_url.contains("coding-intl.dashscope.aliyuncs.com");
        let curl_auth = needs_curl.then(|| format!("Authorization: Bearer {api_key}"));

        let http_client = Arc::new({
            let client = match adapter {
                Some(a) => AdaptedClient::with_adapter(raw_http_client, a),
                None => AdaptedClient::new(raw_http_client),
            };
            if let Some(auth) = curl_auth {
                client.with_curl_transport(auth)
            } else {
                client
            }
        });

        // Check model capabilities via models.dev metadata
        let (supports_temperature, model_max_tokens, model_context_length) = {
            let model_info = registry.find_model_by_id(&config.model);
            let supports_temp = model_info
                .map(|(_, _, m)| m.supports_temperature)
                .unwrap_or(true);
            let max_tok = model_info
                .and_then(|(_, _, m)| m.max_tokens)
                .unwrap_or(config.max_tokens as u64);
            let ctx_len = model_info
                .map(|(_, _, m)| m.context_length)
                .filter(|&v| v > 0)
                .unwrap_or(config.max_context_tokens);
            (supports_temp, max_tok, ctx_len)
        };

        // Create debug logger early so it can be shared with subagent tool
        let debug_logger = Arc::new(if config.debug_logging {
            let session_id = session_manager
                .current_session()
                .map(|s| s.id.as_str())
                .unwrap_or("unknown");
            SessionDebugLogger::new(session_manager.session_dir(), session_id)
        } else {
            SessionDebugLogger::noop()
        });

        // Register SpawnSubagentTool now that we have Arc<ToolRegistry> and Arc<HttpClient>
        let session_dir = session_manager.session_dir().to_path_buf();
        let mut subagent_manager =
            opendev_agents::SubagentManager::with_builtins_and_custom_git_root(
                working_dir,
                git_root.as_deref(),
            );
        // Apply inline agent config overrides from opendev.json
        if !config.agents.is_empty() {
            subagent_manager.apply_config_overrides(&config.agents);
            info!(
                overrides = config.agents.len(),
                "Applied inline agent config overrides"
            );
        }
        let subagent_manager = Arc::new(subagent_manager);
        // Create subagent event channel for TUI bridging
        let (subagent_event_tx, subagent_event_rx) =
            tokio::sync::mpsc::unbounded_channel::<opendev_tools_impl::SubagentEvent>();
        tool_registry.register(Arc::new(
            SpawnSubagentTool::new(
                subagent_manager,
                Arc::clone(&tool_registry),
                Arc::clone(&http_client),
                session_dir,
                &config.model,
                working_dir.display().to_string(),
            )
            .with_event_sender(subagent_event_tx)
            .with_parent_max_tokens(model_max_tokens)
            .with_parent_reasoning_effort(if config.reasoning_effort == "none" {
                None
            } else {
                Some(config.reasoning_effort.clone())
            })
            .with_debug_logger(Arc::clone(&debug_logger)),
        ));
        channel_receivers.subagent_event_rx = Some(subagent_event_rx);
        info!(
            tool_count = tool_registry.tool_names().len(),
            "Registered all tools including spawn_subagent"
        );

        // Configure LLM caller
        let llm_caller = LlmCaller::new(LlmCallConfig {
            model: config.model.clone(),
            temperature: if supports_temperature {
                Some(config.temperature)
            } else {
                None
            },
            max_tokens: Some(model_max_tokens),
            reasoning_effort: if config.reasoning_effort == "none" {
                None
            } else {
                Some(config.reasoning_effort.clone())
            },
        });

        let react_loop = ReactLoop::new(ReactLoopConfig::default());

        let cost_tracker = Mutex::new(CostTracker::new());
        let artifact_index = Mutex::new(ArtifactIndex::new());
        let compactor = Mutex::new(ContextCompactor::new(model_context_length));
        let topic_detector = TopicDetector::new(&provider);

        // Create per-turn prompt composer with section caching
        let (prompt_composer, prompt_context) = tools::create_prompt_composer(working_dir, &config);

        Ok(Self {
            config,
            working_dir: working_dir.to_path_buf(),
            session_manager,
            tool_registry,
            handler_registry,
            query_enhancer,
            http_client,
            llm_caller,
            react_loop,
            cost_tracker,
            artifact_index,
            compactor,
            todo_manager,
            tool_approval_tx: Some(tool_approval_tx),
            channel_receivers: Some(channel_receivers),
            mcp_manager: None,
            skill_loader,
            topic_detector,
            snapshot_manager: Arc::new(Mutex::new(opendev_history::SnapshotManager::new(
                &working_dir.to_string_lossy(),
            ))),
            debug_logger,
            prompt_composer: Mutex::new(prompt_composer),
            prompt_context: Mutex::new(prompt_context),
        })
    }
}

impl AgentRuntime {
    /// Compose the system prompt for the current turn.
    ///
    /// Uses the section cache: `Static` sections are resolved once per session,
    /// `Cached` sections are resolved once until `/clear` or `/compact`,
    /// `Uncached` sections are resolved fresh every call.
    pub fn compose_system_prompt(&self) -> String {
        let mut composer = self.prompt_composer.lock().expect("prompt_composer lock");
        let context = self.prompt_context.lock().expect("prompt_context lock");
        composer.compose(&context)
    }

    /// Pre-resolve MCP instructions and inject as an override before composing.
    ///
    /// Call this before `compose_system_prompt()` when MCP servers may have
    /// connected or disconnected since the last turn.
    pub async fn resolve_mcp_instructions(&self) {
        let mcp_instructions = if let Some(ref mgr) = self.mcp_manager {
            let schemas = mgr.get_all_tool_schemas().await;
            if schemas.is_empty() {
                None
            } else {
                let mut parts = vec![
                    "# MCP Server Instructions\n\nThe following tools are provided by MCP servers:"
                        .to_string(),
                ];
                for schema in &schemas {
                    parts.push(format!("- **{}**: {}", schema.name, schema.description));
                }
                Some(parts.join("\n"))
            }
        } else {
            None
        };

        if let Ok(mut composer) = self.prompt_composer.lock() {
            composer.set_section_override("mcp_instructions", mcp_instructions);
        }
    }

    /// Clear `Cached` section entries. Called on `/compact` and `/clear`.
    pub fn clear_prompt_cache(&self) {
        if let Ok(mut composer) = self.prompt_composer.lock() {
            composer.clear_cache();
        }
    }

    /// Clear all cache entries including `Static`. Called on session switch.
    #[allow(dead_code)]
    pub fn clear_all_prompt_cache(&self) {
        if let Ok(mut composer) = self.prompt_composer.lock() {
            composer.clear_all_cache();
        }
    }

    /// Switch to a new model, rebuilding the HTTP client if the provider changes.
    ///
    /// Returns the new model name for confirmation, or an error message.
    pub fn switch_model(&mut self, new_model: &str) -> Result<String, String> {
        let old_model = &self.llm_caller.config.model;

        // Look up the new model in the registry
        let paths = opendev_config::Paths::new(Some(self.working_dir.clone()));
        let registry = opendev_config::ModelRegistry::load_from_cache(&paths.global_cache_dir());

        let (new_provider_id, new_model_info) =
            if let Some((provider_id, _key, model_info)) = registry.find_model_by_id(new_model) {
                (provider_id.to_string(), Some(model_info.clone()))
            } else {
                // Model not in registry — allow it but warn; keep current provider
                info!(
                    model = new_model,
                    "Model not found in registry, using as-is"
                );
                self.llm_caller.config.model = new_model.to_string();
                return Ok(new_model.to_string());
            };

        // Detect current provider
        let current_provider = {
            if let Some((pid, _, _)) = registry.find_model_by_id(old_model) {
                pid.to_string()
            } else {
                // Can't determine current provider — force rebuild
                String::new()
            }
        };

        // Update model name
        self.llm_caller.config.model = new_model.to_string();

        // Update model-specific config from registry
        if let Some(ref info) = new_model_info {
            self.llm_caller.config.temperature = if info.supports_temperature {
                Some(self.config.temperature)
            } else {
                None
            };
            if let Some(max_tok) = info.max_tokens {
                self.llm_caller.config.max_tokens = Some(max_tok);
            }
            // Update compactor max context for new model's context window
            if info.context_length > 0
                && let Ok(mut comp) = self.compactor.lock()
            {
                comp.set_max_context(info.context_length);
            }
        }

        // Reset reasoning effort: new model may not support the current level.
        // User can re-enable via Ctrl+Shift+T.
        if !new_model_info
            .as_ref()
            .is_some_and(|info| info.capabilities.iter().any(|c| c == "reasoning"))
        {
            self.llm_caller.config.reasoning_effort = None;
        }

        // If provider changed, rebuild the HTTP client
        if new_provider_id != current_provider {
            let provider_info = registry.get_provider(&new_provider_id);
            let registry_env = provider_info.map(|pi| pi.api_key_env.as_str());
            let api_key = self
                .config
                .get_api_key_with_env(registry_env)
                .unwrap_or_default();

            if api_key.is_empty() {
                let env_hint = registry_env.filter(|s| !s.is_empty()).unwrap_or("API_KEY");
                return Err(format!(
                    "No API key for provider '{}'. Set {} environment variable.",
                    new_provider_id, env_hint
                ));
            }

            let base_url = provider_info
                .map(|pi| pi.api_base_url.clone())
                .filter(|s| !s.is_empty());
            let new_client = Self::build_http_client(
                &new_provider_id,
                &api_key,
                new_model,
                base_url.as_deref(),
            )?;
            self.http_client = Arc::new(new_client);
            info!(
                provider = %new_provider_id,
                model = new_model,
                "Rebuilt HTTP client for new provider"
            );
        }

        Ok(new_model.to_string())
    }

    /// Build an HTTP client for a given provider and model.
    fn build_http_client(
        provider: &str,
        api_key: &str,
        model: &str,
        api_base_url: Option<&str>,
    ) -> Result<AdaptedClient, String> {
        let (api_url, headers, adapter): (String, HeaderMap, Option<Box<dyn ProviderAdapter>>) =
            match provider {
                "anthropic" => {
                    let adapter = opendev_http::adapters::anthropic::AnthropicAdapter::new();
                    let url = api_base_url
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| adapter.api_url().to_string());
                    let mut hdrs = HeaderMap::new();
                    if let Ok(val) = HeaderValue::from_str(api_key) {
                        hdrs.insert("x-api-key", val);
                    }
                    hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    for (key, value) in adapter.extra_headers() {
                        if let (Ok(k), Ok(v)) = (
                            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                            HeaderValue::from_str(&value),
                        ) {
                            hdrs.insert(k, v);
                        }
                    }
                    (
                        url,
                        hdrs,
                        Some(Box::new(adapter) as Box<dyn ProviderAdapter>),
                    )
                }
                "openai" => {
                    let adapter = opendev_http::adapters::openai::OpenAiAdapter::new();
                    let url = api_base_url
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| adapter.api_url().to_string());
                    let mut hdrs = HeaderMap::new();
                    if let Ok(val) = HeaderValue::from_str(&format!("Bearer {api_key}")) {
                        hdrs.insert(AUTHORIZATION, val);
                    }
                    hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    (
                        url,
                        hdrs,
                        Some(Box::new(adapter) as Box<dyn ProviderAdapter>),
                    )
                }
                "gemini" | "google" => {
                    let adapter = opendev_http::adapters::gemini::GeminiAdapter::new(model);
                    let api_url = api_base_url
                        .map(|base| opendev_http::adapters::gemini::gemini_api_url(base, model))
                        .unwrap_or_else(|| {
                            opendev_http::adapters::gemini::gemini_api_url(adapter.api_url(), model)
                        });
                    let mut hdrs = HeaderMap::new();
                    if let Ok(val) = HeaderValue::from_str(api_key) {
                        hdrs.insert("x-goog-api-key", val);
                    }
                    hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    (
                        api_url,
                        hdrs,
                        Some(Box::new(adapter) as Box<dyn ProviderAdapter>),
                    )
                }
                "ollama" => {
                    let adapter = opendev_http::adapters::ollama::OllamaAdapter::new();
                    let url = api_base_url
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| adapter.api_url().to_string());
                    let mut hdrs = HeaderMap::new();
                    hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    (
                        url,
                        hdrs,
                        Some(Box::new(adapter) as Box<dyn ProviderAdapter>),
                    )
                }
                "azure" => {
                    let base = api_base_url.unwrap_or("https://api.openai.com");
                    let url = format!(
                        "{}/openai/deployments/{model}/chat/completions?api-version=2024-10-21",
                        base.trim_end_matches('/')
                    );
                    let mut hdrs = HeaderMap::new();
                    if let Ok(val) = HeaderValue::from_str(api_key) {
                        hdrs.insert("api-key", val);
                    }
                    hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    let adapter =
                        opendev_http::adapters::chat_completions::ChatCompletionsAdapter::new(
                            url.clone(),
                        );
                    (
                        url,
                        hdrs,
                        Some(Box::new(adapter) as Box<dyn ProviderAdapter>),
                    )
                }
                _ => {
                    // OpenAI-compatible providers — use registry api_base_url
                    let url = api_base_url
                        .map(|base| {
                            let trimmed = base.trim_end_matches('/');
                            if trimmed.ends_with("/chat/completions") {
                                trimmed.to_string()
                            } else {
                                format!("{trimmed}/chat/completions")
                            }
                        })
                        .unwrap_or_else(|| {
                            "https://api.openai.com/v1/chat/completions".to_string()
                        });
                    let mut hdrs = HeaderMap::new();
                    if let Ok(val) = HeaderValue::from_str(&format!("Bearer {api_key}")) {
                        hdrs.insert(AUTHORIZATION, val);
                    }
                    hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    if provider == "openrouter"
                        && let Ok(val) = HeaderValue::from_str("https://opendev.ai")
                    {
                        hdrs.insert("HTTP-Referer", val);
                    }
                    let adapter =
                        opendev_http::adapters::chat_completions::ChatCompletionsAdapter::new(
                            url.clone(),
                        );
                    (
                        url,
                        hdrs,
                        Some(Box::new(adapter) as Box<dyn ProviderAdapter>),
                    )
                }
            };

        let circuit_breaker =
            std::sync::Arc::new(opendev_http::CircuitBreaker::with_defaults(provider));
        let needs_curl = api_url.contains("coding-intl.dashscope.aliyuncs.com");
        let raw = HttpClient::new(api_url, headers, None)
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?
            .with_circuit_breaker(circuit_breaker);

        let client = match adapter {
            Some(a) => AdaptedClient::with_adapter(raw, a),
            None => AdaptedClient::new(raw),
        };
        // DashScope coding endpoint rejects reqwest — attach curl subprocess transport.
        let client = if needs_curl {
            client.with_curl_transport(format!("Authorization: Bearer {api_key}"))
        } else {
            client
        };
        Ok(client)
    }
}

impl std::fmt::Debug for AgentRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentRuntime")
            .field("working_dir", &self.working_dir)
            .field("model", &self.llm_caller.config.model)
            .finish()
    }
}

#[cfg(test)]
mod tests;
