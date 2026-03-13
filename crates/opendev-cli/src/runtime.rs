//! Agent runtime — central orchestration struct.
//!
//! Owns all services and coordinates the full pipeline:
//! CLI → REPL → QueryEnhancer → ReactLoop → ToolExecutor → display

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::Value;
use tracing::{debug, info, warn};

use opendev_agents::llm_calls::{LlmCallConfig, LlmCaller};
use opendev_agents::prompts::{create_default_composer, create_thinking_composer};
use opendev_agents::react_loop::{ReactLoop, ReactLoopConfig};
use opendev_agents::traits::{AgentError, AgentEventCallback, AgentResult, TaskMonitor};
use opendev_history::SessionManager;
use opendev_http::HttpClient;
use opendev_http::adapted_client::AdaptedClient;
use opendev_http::adapters::base::ProviderAdapter;
use opendev_models::AppConfig;
use opendev_models::message::{ChatMessage, Role};
use opendev_repl::HandlerRegistry;
use opendev_repl::query_enhancer::QueryEnhancer;
use opendev_runtime::CostTracker;
use opendev_tools_core::{ToolContext, ToolRegistry};
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
    /// Tool registry with all available tools.
    pub tool_registry: ToolRegistry,
    /// Handler middleware for pre/post tool processing.
    pub handler_registry: HandlerRegistry,
    /// Query enhancer for @ file injection and message preparation.
    pub query_enhancer: QueryEnhancer,
    /// HTTP client for LLM API calls (with provider adapter).
    pub http_client: AdaptedClient,
    /// LLM caller configuration.
    pub llm_caller: LlmCaller,
    /// ReAct loop.
    pub react_loop: ReactLoop,
    /// Cost tracker.
    pub cost_tracker: CostTracker,
}

/// Register all built-in tools into the registry.
fn register_default_tools(registry: &mut ToolRegistry) {
    // Process execution
    registry.register(Arc::new(BashTool::new()));

    // File operations
    registry.register(Arc::new(FileReadTool));
    registry.register(Arc::new(FileWriteTool));
    registry.register(Arc::new(FileEditTool));
    registry.register(Arc::new(FileListTool));
    registry.register(Arc::new(FileSearchTool));

    // Git
    registry.register(Arc::new(GitTool));
    registry.register(Arc::new(PatchTool));

    // Web tools
    registry.register(Arc::new(WebFetchTool));
    registry.register(Arc::new(WebSearchTool));
    registry.register(Arc::new(WebScreenshotTool));
    registry.register(Arc::new(BrowserTool));
    registry.register(Arc::new(OpenBrowserTool));

    // User interaction
    registry.register(Arc::new(AskUserTool));

    // Memory & session
    registry.register(Arc::new(MemoryTool));
    registry.register(Arc::new(SessionTool));
    registry.register(Arc::new(MessageTool));

    // Scheduling & misc
    registry.register(Arc::new(ScheduleTool));
    registry.register(Arc::new(PdfTool));
    registry.register(Arc::new(BatchTool));
    registry.register(Arc::new(NotebookEditTool));
    registry.register(Arc::new(TaskCompleteTool));
    registry.register(Arc::new(VlmTool));
    registry.register(Arc::new(DiffPreviewTool));
    registry.register(Arc::new(PresentPlanTool::new()));

    // Todo (with shared manager)
    let todo_manager = Arc::new(Mutex::new(opendev_runtime::TodoManager::new()));
    registry.register(Arc::new(TodoTool::new(todo_manager)));

    // Agent tools
    registry.register(Arc::new(AgentsTool));
    // Note: SpawnSubagentTool requires shared Arc<ToolRegistry> and Arc<HttpClient>,
    // which are created after registration. Deferred for now.
}

/// Build the system prompt from embedded templates.
pub fn build_system_prompt(working_dir: &Path, config: &AppConfig) -> String {
    // Use a dummy path — templates are resolved from the embedded store first
    let composer = create_default_composer("/dev/null");

    let mut context = HashMap::new();
    context.insert(
        "model".to_string(),
        serde_json::Value::String(config.model.clone()),
    );
    context.insert(
        "working_dir".to_string(),
        serde_json::Value::String(working_dir.display().to_string()),
    );
    context.insert(
        "is_git_repo".to_string(),
        serde_json::Value::Bool(working_dir.join(".git").exists()),
    );

    composer.compose(&context)
}

impl AgentRuntime {
    /// Create a new agent runtime with all tools registered.
    pub fn new(
        config: AppConfig,
        working_dir: &Path,
        session_manager: SessionManager,
    ) -> Result<Self, String> {
        let mut tool_registry = ToolRegistry::new();
        register_default_tools(&mut tool_registry);
        info!(
            tool_count = tool_registry.tool_names().len(),
            "Registered default tools"
        );

        let handler_registry = HandlerRegistry::new();
        let query_enhancer = QueryEnhancer::new(working_dir.to_path_buf());

        // Configure HTTP client based on provider
        let api_key = config.get_api_key().unwrap_or_default();

        let (api_url, headers, adapter): (
            String,
            HeaderMap,
            Option<Box<dyn opendev_http::adapters::base::ProviderAdapter>>,
        ) = match config.model_provider.as_str() {
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

        let raw_http_client = HttpClient::new(api_url, headers, None)
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

        let http_client = match adapter {
            Some(a) => AdaptedClient::with_adapter(raw_http_client, a),
            None => AdaptedClient::new(raw_http_client),
        };

        // Check if model supports temperature via models.dev metadata
        let supports_temperature = {
            let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
            let registry =
                opendev_config::ModelRegistry::load_from_cache(&paths.global_cache_dir());
            registry
                .find_model_by_id(&config.model)
                .map(|(_, _, m)| m.supports_temperature)
                .unwrap_or(true)
        };

        // Configure LLM caller
        let llm_caller = LlmCaller::new(LlmCallConfig {
            model: config.model.clone(),
            temperature: if supports_temperature {
                Some(config.temperature)
            } else {
                None
            },
            max_tokens: Some(config.max_tokens as u64),
        });

        let react_loop = ReactLoop::new(ReactLoopConfig::default());

        let cost_tracker = CostTracker::new();

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
        })
    }

    /// Run a single query through the full pipeline.
    ///
    /// Pipeline: enhance query → save user message → prepare messages →
    ///           ReactLoop → save assistant response → return result
    pub async fn run_query(
        &mut self,
        query: &str,
        system_prompt: &str,
        event_callback: Option<&dyn AgentEventCallback>,
    ) -> Result<AgentResult, AgentError> {
        info!(
            query_len = query.len(),
            "Running query through agent pipeline"
        );

        // Step 1: Save user message to session
        if let Some(session) = self.session_manager.current_session_mut() {
            session.messages.push(ChatMessage {
                role: Role::User,
                content: query.to_string(),
                timestamp: Utc::now(),
                metadata: HashMap::new(),
                tool_calls: Vec::new(),
                tokens: None,
                thinking_trace: None,
                reasoning_content: None,
                token_usage: None,
                provenance: None,
            });
        }

        // Step 2: Enhance query with @ file references
        let (enhanced_query, image_blocks) = self.query_enhancer.enhance_query(query);
        debug!(
            enhanced_len = enhanced_query.len(),
            image_count = image_blocks.len(),
            "Query enhanced"
        );

        // Step 3: Prepare messages (session history + system prompt + enhanced query)
        let session_messages = self
            .session_manager
            .current_session()
            .map(|s| {
                s.messages
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "role": m.role.to_string(),
                            "content": &m.content,
                        })
                    })
                    .collect::<Vec<Value>>()
            })
            .unwrap_or_default();

        let mut messages = self.query_enhancer.prepare_messages(
            query,
            &enhanced_query,
            system_prompt,
            Some(&session_messages),
            &image_blocks,
            false, // thinking_visible
            None,  // playbook_context
        );

        // Step 4: Get tool schemas for the LLM
        let tool_schemas = self.tool_registry.get_schemas();

        // Step 5: Create tool context
        let tool_context = ToolContext {
            working_dir: self.working_dir.clone(),
            is_subagent: false,
            session_id: self.session_manager.current_session().map(|s| s.id.clone()),
            values: HashMap::new(),
        };

        // Step 6: Set thinking context for this query
        let thinking_sys_prompt = {
            let composer = create_thinking_composer("/dev/null");
            let prompt = composer.compose(&HashMap::new());
            if prompt.is_empty() { None } else { Some(prompt) }
        };
        self.react_loop
            .set_thinking_context(Some(query.to_string()), thinking_sys_prompt);

        // Step 7: Run the ReAct loop
        let result = self
            .react_loop
            .run(
                &self.llm_caller,
                &self.http_client,
                &mut messages,
                &tool_schemas,
                &self.tool_registry,
                &tool_context,
                None::<&dyn TaskMonitor>,
                event_callback,
            )
            .await?;

        // Step 7: Save assistant response to session
        if let Some(session) = self.session_manager.current_session_mut() {
            session.messages.push(ChatMessage {
                role: Role::Assistant,
                content: result.content.clone(),
                timestamp: Utc::now(),
                metadata: HashMap::new(),
                tool_calls: Vec::new(),
                tokens: None,
                thinking_trace: None,
                reasoning_content: None,
                token_usage: None,
                provenance: None,
            });
        }

        // Step 8: Persist session to disk
        if let Err(e) = self.session_manager.save_current() {
            warn!("Failed to save session: {e}");
        }

        info!(
            success = result.success,
            content_len = result.content.len(),
            "Query completed"
        );

        Ok(result)
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
