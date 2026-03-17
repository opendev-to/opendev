//! Agent runtime — central orchestration struct.
//!
//! Owns all services and coordinates the full pipeline:
//! CLI → REPL → QueryEnhancer → ReactLoop → ToolExecutor → display

mod query;
mod tools;

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
use opendev_runtime::CostTracker;
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
    pub(super) snapshot_manager: Mutex<opendev_history::SnapshotManager>,
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

        // BatchTool needs Arc<ToolRegistry> for dispatching calls.
        tool_registry.register(Arc::new(BatchTool::new(Arc::clone(&tool_registry))));

        // Register custom tools from .opendev/tools/ and .opencode/tool/ directories.
        let custom_tools = opendev_tools_impl::custom_tool::discover_custom_tools(working_dir);
        for tool in custom_tools {
            info!(name = tool.name(), "Registered custom tool");
            tool_registry.register(Arc::new(tool));
        }

        // Register invoke_skill tool with project-local and user-global skill dirs.
        // Priority order (first = highest): .claude/skills > .agents/skills > .opendev/skills
        // at each level from working_dir up to git root, then global dirs,
        // then config-specified skill_paths (lowest priority among custom dirs).
        let mut skill_dirs = Vec::new();

        // Walk from working_dir up to git root, scanning for skill directories
        // at each level. This supports monorepos where subdirectories can have
        // their own skill overrides.
        let git_root = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(working_dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| PathBuf::from(s.trim()))
            });
        let stop_dir = git_root.as_deref().unwrap_or(working_dir);

        {
            let mut current = working_dir.to_path_buf();
            loop {
                for subdir in &[".claude", ".agents", ".opendev"] {
                    let skills_dir = current.join(subdir).join("skills");
                    skill_dirs.push(skills_dir);
                }
                if current == stop_dir || !current.pop() {
                    break;
                }
            }
        }

        // Global (home) skill directories
        if let Some(home) = dirs_next::home_dir() {
            for subdir in &[".claude", ".agents", ".opendev"] {
                skill_dirs.push(home.join(subdir).join("skills"));
            }
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

        // Configure HTTP client based on provider
        let api_key = config.get_api_key().unwrap_or_default();

        // Auto-detect provider from API key if not explicitly set
        let provider = AdaptedClient::resolve_provider(&config.model_provider, &api_key);
        debug!(provider = %provider, "Resolved model provider");

        let (api_url, headers, adapter): (
            String,
            HeaderMap,
            Option<Box<dyn opendev_http::adapters::base::ProviderAdapter>>,
        ) = match provider.as_str() {
            "anthropic" => {
                let adapter = opendev_http::adapters::anthropic::AnthropicAdapter::new();
                let url = config
                    .api_base_url
                    .clone()
                    .unwrap_or_else(|| adapter.api_url().to_string());
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
                let url = config
                    .api_base_url
                    .clone()
                    .unwrap_or_else(|| adapter.api_url().to_string());
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
            "gemini" | "google" => {
                let adapter = opendev_http::adapters::gemini::GeminiAdapter::new(&config.model);
                let api_url = config
                    .api_base_url
                    .clone()
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
                let base = config
                    .api_base_url
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
                (url, hdrs, None)
            }
            provider => {
                // All other OpenAI-compatible providers
                let url = match provider {
                    "fireworks" => "https://api.fireworks.ai/inference/v1/chat/completions",
                    "groq" => "https://api.groq.com/openai/v1/chat/completions",
                    "mistral" => "https://api.mistral.ai/v1/chat/completions",
                    "deepinfra" => "https://api.deepinfra.com/v1/openai/chat/completions",
                    "openrouter" => "https://openrouter.ai/api/v1/chat/completions",
                    _ => {
                        // Custom provider: use api_base_url or default to OpenAI
                        ""
                    }
                };
                let url = if url.is_empty() {
                    config
                        .api_base_url
                        .clone()
                        .map(|base| {
                            let trimmed = base.trim_end_matches('/');
                            if trimmed.ends_with("/chat/completions") {
                                trimmed.to_string()
                            } else {
                                format!("{trimmed}/chat/completions")
                            }
                        })
                        .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string())
                } else {
                    url.to_string()
                };

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
                (url, hdrs, None)
            }
        };

        let circuit_breaker =
            std::sync::Arc::new(opendev_http::CircuitBreaker::with_defaults(&provider));
        let raw_http_client = HttpClient::new(api_url, headers, None)
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?
            .with_circuit_breaker(circuit_breaker);

        let http_client = Arc::new(match adapter {
            Some(a) => AdaptedClient::with_adapter(raw_http_client, a),
            None => AdaptedClient::new(raw_http_client),
        });

        // Check model capabilities via models.dev metadata
        let (supports_temperature, model_max_tokens) = {
            let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
            let registry =
                opendev_config::ModelRegistry::load_from_cache(&paths.global_cache_dir());
            let model_info = registry.find_model_by_id(&config.model);
            let supports_temp = model_info
                .map(|(_, _, m)| m.supports_temperature)
                .unwrap_or(true);
            let max_tok = model_info
                .and_then(|(_, _, m)| m.max_tokens)
                .unwrap_or(config.max_tokens as u64);
            (supports_temp, max_tok)
        };

        // Register SpawnSubagentTool now that we have Arc<ToolRegistry> and Arc<HttpClient>
        let session_dir = session_manager.session_dir().to_path_buf();
        let mut subagent_manager =
            opendev_agents::SubagentManager::with_builtins_and_custom(working_dir);
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
            .with_tool_approval_tx(tool_approval_tx.clone())
            .with_parent_max_tokens(model_max_tokens),
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
        });

        let react_loop = ReactLoop::new(ReactLoopConfig::default());

        let cost_tracker = Mutex::new(CostTracker::new());
        let artifact_index = Mutex::new(ArtifactIndex::new());
        let compactor = Mutex::new(ContextCompactor::new(config.max_context_tokens));
        let topic_detector = TopicDetector::new(&provider);

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
            snapshot_manager: Mutex::new(opendev_history::SnapshotManager::new(
                &working_dir.to_string_lossy(),
            )),
        })
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
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions");
        std::fs::create_dir_all(&session_dir).unwrap();
        let sm = SessionManager::new(session_dir).unwrap();
        let config = AppConfig::default();

        let runtime = AgentRuntime::new(config, tmp.path(), sm);
        assert!(runtime.is_ok());
        let rt = runtime.unwrap();
        // Should have tools registered
        assert!(rt.tool_registry.tool_names().len() > 20);
    }

    #[test]
    fn test_runtime_debug_format() {
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions");
        std::fs::create_dir_all(&session_dir).unwrap();
        let sm = SessionManager::new(session_dir).unwrap();
        let config = AppConfig::default();

        let runtime = AgentRuntime::new(config, tmp.path(), sm).unwrap();
        let debug = format!("{:?}", runtime);
        assert!(debug.contains("AgentRuntime"));
    }

    #[test]
    fn test_build_system_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let prompt = build_system_prompt(tmp.path(), &config);
        // Should produce a non-trivial prompt from embedded templates
        assert!(!prompt.is_empty());
    }
}
