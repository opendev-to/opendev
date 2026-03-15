//! Agent runtime — central orchestration struct.
//!
//! Owns all services and coordinates the full pipeline:
//! CLI → REPL → QueryEnhancer → ReactLoop → ToolExecutor → display

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use tracing::{debug, info, warn};

use opendev_agents::llm_calls::{LlmCallConfig, LlmCaller};
use opendev_agents::prompts::{create_default_composer, create_thinking_composer};
use opendev_agents::react_loop::{ReactLoop, ReactLoopConfig};
use opendev_agents::traits::{AgentError, AgentEventCallback, AgentResult};
use opendev_context::{ArtifactIndex, ContextCompactor};
use opendev_history::SessionManager;
use opendev_history::topic_detector::{SimpleMessage, TopicDetector};
use opendev_http::HttpClient;
use opendev_http::adapted_client::AdaptedClient;
use opendev_http::adapters::base::ProviderAdapter;
use opendev_mcp::McpManager;
use opendev_models::AppConfig;
use opendev_models::message::{ChatMessage, Role};
use opendev_repl::HandlerRegistry;
use opendev_repl::query_enhancer::QueryEnhancer;
use opendev_runtime::CostTracker;
use opendev_tools_core::{BaseTool, ToolContext, ToolRegistry};
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
    skill_loader: Arc<Mutex<opendev_agents::SkillLoader>>,
    /// LLM-based topic detector for auto-generating session titles.
    topic_detector: TopicDetector,
}

/// Receivers returned from tool registration for TUI bridging.
pub struct ToolChannelReceivers {
    pub ask_user_rx: opendev_runtime::AskUserReceiver,
    pub plan_approval_rx: opendev_runtime::PlanApprovalReceiver,
    pub tool_approval_rx: opendev_runtime::ToolApprovalReceiver,
    pub subagent_event_rx:
        Option<tokio::sync::mpsc::UnboundedReceiver<opendev_tools_impl::SubagentEvent>>,
}

/// Register all built-in tools into the registry.
fn register_default_tools(
    registry: &ToolRegistry,
) -> (
    Arc<Mutex<opendev_runtime::TodoManager>>,
    ToolChannelReceivers,
    opendev_runtime::ToolApprovalSender,
) {
    // Process execution
    registry.register(Arc::new(BashTool::new()));

    // File operations
    registry.register(Arc::new(FileReadTool));
    registry.register(Arc::new(FileWriteTool));
    registry.register(Arc::new(FileEditTool));
    registry.register(Arc::new(MultiEditTool));
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

    // User interaction — with channel for TUI mode
    let (ask_user_tx, ask_user_rx) = opendev_runtime::ask_user_channel();
    registry.register(Arc::new(AskUserTool::new().with_ask_tx(ask_user_tx)));

    // Memory & session
    registry.register(Arc::new(MemoryTool));
    registry.register(Arc::new(SessionTool));
    registry.register(Arc::new(MessageTool));

    // Scheduling & misc
    registry.register(Arc::new(ScheduleTool));
    // BatchTool is registered later (needs Arc<ToolRegistry> for dispatch).
    registry.register(Arc::new(NotebookEditTool));
    registry.register(Arc::new(TaskCompleteTool));
    registry.register(Arc::new(VlmTool));
    registry.register(Arc::new(DiffPreviewTool));
    // Plan tool — with channel for TUI approval
    let (plan_approval_tx, plan_approval_rx) = opendev_runtime::plan_approval_channel();
    registry.register(Arc::new(
        PresentPlanTool::new().with_approval_tx(plan_approval_tx),
    ));

    // Todo tools (5 separate tools sharing one manager)
    let todo_manager = Arc::new(Mutex::new(opendev_runtime::TodoManager::new()));
    registry.register(Arc::new(WriteTodosTool::new(Arc::clone(&todo_manager))));
    registry.register(Arc::new(UpdateTodoTool::new(Arc::clone(&todo_manager))));
    registry.register(Arc::new(CompleteTodoTool::new(Arc::clone(&todo_manager))));
    registry.register(Arc::new(ListTodosTool::new(Arc::clone(&todo_manager))));
    registry.register(Arc::new(ClearTodosTool::new(Arc::clone(&todo_manager))));
    // Keep legacy single-action tool for backward compatibility
    registry.register(Arc::new(TodoTool::new(Arc::clone(&todo_manager))));

    // Agent tools
    registry.register(Arc::new(AgentsTool));
    // Note: SpawnSubagentTool requires shared Arc<ToolRegistry> and Arc<HttpClient>,
    // which are created after registration. Deferred for now.

    // Tool approval channel (sender stored on runtime for react loop, receiver goes to TUI)
    let (tool_approval_tx, tool_approval_rx) = opendev_runtime::tool_approval_channel();

    (
        todo_manager,
        ToolChannelReceivers {
            ask_user_rx,
            plan_approval_rx,
            tool_approval_rx,
            subagent_event_rx: None, // Populated later when SpawnSubagentTool is registered
        },
        tool_approval_tx,
    )
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

    let base_prompt = composer.compose(&context);

    // Collect and append dynamic environment context
    let mut env_ctx = opendev_context::EnvironmentContext::collect(working_dir);

    // Resolve config-level instruction paths (file paths, globs, ~/paths)
    if !config.instructions.is_empty() {
        let config_instructions =
            opendev_context::resolve_instruction_paths(&config.instructions, working_dir);
        // Deduplicate against already-discovered files
        let existing: std::collections::HashSet<_> = env_ctx
            .instruction_files
            .iter()
            .filter_map(|f| f.path.canonicalize().ok())
            .collect();
        for instr in config_instructions {
            if let Ok(canonical) = instr.path.canonicalize()
                && !existing.contains(&canonical)
            {
                env_ctx.instruction_files.push(instr);
            }
        }
    }

    let env_block = env_ctx.format_prompt_block();

    if env_block.is_empty() {
        base_prompt
    } else {
        format!("{base_prompt}\n\n{env_block}")
    }
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
            register_default_tools(&tool_registry);

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
            .with_event_sender(subagent_event_tx),
        ));
        // Attach subagent event receiver to channel_receivers for TUI bridging
        channel_receivers.subagent_event_rx = Some(subagent_event_rx);
        info!(
            tool_count = tool_registry.tool_names().len(),
            "Registered all tools including spawn_subagent"
        );

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
        })
    }

    /// Connect to configured MCP servers and register their tools.
    ///
    /// Loads MCP config from global (`~/.opendev/mcp.json`) and project
    /// (`.mcp.json`) files, connects to all enabled servers, discovers
    /// tools, and registers them as `McpBridgeTool` instances.
    ///
    /// Failures are logged but do not prevent the runtime from starting —
    /// MCP is optional and best-effort.
    pub async fn connect_mcp_servers(&mut self) {
        let manager = Arc::new(McpManager::new(Some(self.working_dir.clone())));

        // Load configuration from disk
        if let Err(e) = manager.load_configuration().await {
            debug!(error = %e, "No MCP config loaded (this is normal if no MCP servers are configured)");
            return;
        }

        // Connect all configured servers
        if let Err(e) = manager.connect_all().await {
            warn!(error = %e, "Failed to connect MCP servers");
        }

        // Discover tool schemas from connected servers
        let schemas = manager.get_all_tool_schemas().await;
        if schemas.is_empty() {
            debug!("No MCP tools discovered");
            return;
        }

        // Register each MCP tool as a BaseTool in the registry
        let mut registered = 0;
        for schema in &schemas {
            let bridge = McpBridgeTool::from_schema(schema, Arc::clone(&manager));
            self.tool_registry.register(Arc::new(bridge));
            registered += 1;
        }

        info!(
            mcp_tools = registered,
            total_tools = self.tool_registry.tool_names().len(),
            "Registered MCP tools"
        );

        // Re-register invoke_skill with MCP prompt support.
        self.tool_registry
            .register(Arc::new(InvokeSkillTool::with_mcp(
                Arc::clone(&self.skill_loader),
                Arc::clone(&manager),
            )));

        self.mcp_manager = Some(manager);
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
        interrupt_token: Option<&opendev_runtime::InterruptToken>,
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
            .map(|s| opendev_history::message_convert::chatmessages_to_api_values(&s.messages))
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
            timeout_config: None,
            cancel_token: interrupt_token.map(|t| t.child_token()),
            diagnostic_provider: None,
        };

        // Step 6: Set thinking context for this query
        let thinking_sys_prompt = {
            let composer = create_thinking_composer("/dev/null");
            let prompt = composer.compose(&HashMap::new());
            if prompt.is_empty() {
                None
            } else {
                Some(prompt)
            }
        };
        self.react_loop
            .set_thinking_context(Some(query.to_string()), thinking_sys_prompt);

        // Step 7: Run the ReAct loop
        let pre_count = messages.len();
        // Use interrupt_token as both TaskMonitor and CancellationToken source
        let cancel_token = interrupt_token.map(|t| t.child_token());
        let result = self
            .react_loop
            .run(
                &self.llm_caller,
                &self.http_client,
                &mut messages,
                &tool_schemas,
                &self.tool_registry,
                &tool_context,
                interrupt_token,
                event_callback,
                Some(&self.cost_tracker),
                Some(&self.artifact_index),
                Some(&self.compactor),
                Some(&self.todo_manager),
                cancel_token.as_ref(),
                self.tool_approval_tx.as_ref(),
            )
            .await?;

        // Step 7b: Save all new messages from the react loop to the session.
        // Convert the new API values (assistant + tool messages) back to ChatMessages
        // so tool calls and their results are fully preserved.
        {
            let new_values = &result.messages[pre_count..];
            let new_chat_messages =
                opendev_history::message_convert::api_values_to_chatmessages(new_values);
            for msg in new_chat_messages {
                self.session_manager.add_message(msg);
            }
        }

        // Step 8: Persist session to disk
        if let Err(e) = self.session_manager.save_current() {
            warn!("Failed to save session: {e}");
        }

        // Step 9: Auto-detect session title (only when session has no title yet)
        if self.topic_detector.is_enabled() {
            let needs_title = self
                .session_manager
                .current_session()
                .map(|s| !s.metadata.contains_key("title"))
                .unwrap_or(false);

            if needs_title {
                let simple_msgs: Vec<SimpleMessage> = self
                    .session_manager
                    .current_session()
                    .map(|s| {
                        s.messages
                            .iter()
                            .filter_map(|m| {
                                let role = match m.role {
                                    Role::User => "user",
                                    Role::Assistant => "assistant",
                                    _ => return None,
                                };
                                if m.content.is_empty() {
                                    return None;
                                }
                                Some(SimpleMessage {
                                    role: role.to_string(),
                                    content: m.content.clone(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                if let Some(title) = self.topic_detector.detect_title(&simple_msgs).await
                    && let Some(session) = self.session_manager.current_session()
                {
                    let session_id = session.id.clone();
                    if let Err(e) = self.session_manager.set_title(&session_id, &title) {
                        debug!("Failed to set session title: {e}");
                    } else {
                        self.session_manager.save_current().ok();
                        debug!(title, "Auto-detected session title");
                    }
                }
            }
        }

        // Log session cost
        if let Ok(tracker) = self.cost_tracker.lock() {
            info!(
                cost = tracker.format_cost(),
                calls = tracker.call_count,
                input_tokens = tracker.total_input_tokens,
                output_tokens = tracker.total_output_tokens,
                "Session cost update"
            );
        }

        info!(
            success = result.success,
            content_len = result.content.len(),
            "Query completed"
        );

        Ok(result)
    }

    /// Run manual compaction on the current session's messages.
    ///
    /// Forces LLM-powered compaction regardless of context usage level.
    /// Updates the session messages in-place with the compacted result.
    pub async fn run_compaction(&mut self) -> Result<String, String> {
        use opendev_agents::prompts::embedded::SYSTEM_COMPACTION;

        // Load current session messages as API values
        let session_messages = self
            .session_manager
            .current_session()
            .map(|s| opendev_history::message_convert::chatmessages_to_api_values(&s.messages))
            .unwrap_or_default();

        if session_messages.len() < 5 {
            return Err("Not enough messages to compact (need at least 5).".to_string());
        }

        let api_msgs: Vec<serde_json::Map<String, serde_json::Value>> = session_messages
            .iter()
            .filter_map(|v| v.as_object().cloned())
            .collect();

        let compact_model = &self.llm_caller.config.model;
        let original_count = api_msgs.len();

        // Try LLM-powered compaction
        let build_result = if let Ok(comp) = self.compactor.lock() {
            comp.build_compaction_payload(&api_msgs, SYSTEM_COMPACTION, compact_model)
        } else {
            None
        };

        let mut summary_for_display = String::new();

        let compacted = if let Some((payload, _middle_count, keep_recent)) = build_result {
            // Call LLM for summarization
            let summary_text: Option<String> =
                match self.http_client.post_json(&payload, None).await {
                    Ok(result) => result
                        .body
                        .as_ref()
                        .and_then(|body| body.pointer("/choices/0/message/content"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    Err(e) => {
                        warn!("LLM compaction request failed: {e}, using fallback");
                        None
                    }
                };

            let summary = match summary_text {
                Some(text) if !text.is_empty() => {
                    info!(
                        model = compact_model,
                        summary_len = text.len(),
                        "Manual LLM compaction succeeded"
                    );
                    summary_for_display = text.clone();
                    text
                }
                _ => {
                    warn!("LLM compaction returned empty, using fallback");
                    ContextCompactor::fallback_summary(
                        &api_msgs[1..api_msgs.len().saturating_sub(keep_recent)],
                    )
                }
            };

            if let Ok(mut comp) = self.compactor.lock() {
                comp.apply_llm_compaction(api_msgs, &summary, keep_recent)
            } else {
                return Err("Failed to acquire compactor lock".to_string());
            }
        } else {
            // Fallback to basic compaction
            if let Ok(mut comp) = self.compactor.lock() {
                comp.compact(api_msgs, "")
            } else {
                return Err("Failed to acquire compactor lock".to_string());
            }
        };

        let compacted_count = compacted.len();

        // Convert compacted API messages back to ChatMessages and replace session
        let compacted_values: Vec<serde_json::Value> = compacted
            .into_iter()
            .map(serde_json::Value::Object)
            .collect();
        let new_chat_messages =
            opendev_history::message_convert::api_values_to_chatmessages(&compacted_values);

        if let Some(session) = self.session_manager.current_session_mut() {
            session.messages = new_chat_messages;
        }

        // Save the compacted session
        if let Err(e) = self.session_manager.save_current() {
            warn!("Failed to save compacted session: {e}");
        }

        // Return stats + LLM summary so the TUI can display what was preserved
        let stats = format!(
            "Conversation compacted: {original_count} messages \u{2192} {compacted_count} messages."
        );
        if summary_for_display.is_empty() {
            Ok(stats)
        } else {
            Ok(format!("{stats}\n\n{summary_for_display}"))
        }
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
