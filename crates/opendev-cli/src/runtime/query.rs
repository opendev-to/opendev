//! Query execution pipeline, MCP integration, and context compaction.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tracing::{debug, info, warn};

use opendev_agents::traits::{AgentError, AgentEventCallback, AgentResult};
use opendev_context::ContextCompactor;
use opendev_history::topic_detector::SimpleMessage;
use opendev_mcp::McpManager;
use opendev_models::message::{ChatMessage, Role};
use opendev_tools_core::ToolContext;
use opendev_tools_impl::*;

use super::AgentRuntime;

impl AgentRuntime {
    /// Start MCP server connections in the background.
    ///
    /// Loads MCP config from global (`~/.opendev/mcp.json`) and project
    /// (`.mcp.json`) files, then spawns a background task that connects
    /// to all enabled servers, discovers tools, and registers them as
    /// `McpBridgeTool` instances. Returns immediately so startup is
    /// never blocked by slow or failing MCP servers.
    ///
    /// Failures are logged but do not prevent the runtime from starting —
    /// MCP is optional and best-effort.
    pub fn start_mcp_connections(&mut self) {
        let manager = Arc::new(McpManager::new(Some(self.working_dir.clone())));

        // Store the manager immediately so BackgroundRuntime can reference it,
        // even before connections are established.
        self.mcp_manager = Some(Arc::clone(&manager));

        // Clone Arcs for the background task.
        let tool_registry = Arc::clone(&self.tool_registry);
        let skill_loader = Arc::clone(&self.skill_loader);

        tokio::spawn(async move {
            // Load configuration from disk
            if let Err(e) = manager.load_configuration().await {
                debug!(error = %e, "No MCP config loaded (this is normal if no MCP servers are configured)");
                return;
            }

            // Connect all configured servers (in parallel)
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
                tool_registry.register(Arc::new(bridge));
                registered += 1;
            }

            info!(
                mcp_tools = registered,
                total_tools = tool_registry.tool_names().len(),
                "Registered MCP tools (background)"
            );

            // Re-register invoke_skill with MCP prompt support.
            tool_registry.register(Arc::new(InvokeSkillTool::with_mcp(
                skill_loader,
                Arc::clone(&manager),
            )));
        });
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
        plan_requested: bool,
    ) -> Result<AgentResult, AgentError> {
        info!(
            query_len = query.len(),
            plan_requested, "Running query through agent pipeline"
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
        let shared_state = if plan_requested {
            let plans_dir = dirs_next::home_dir()
                .map(|h: std::path::PathBuf| h.join(".opendev").join("plans"))
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
            let plan_name = opendev_runtime::generate_plan_name(Some(&plans_dir), 50);
            let plan_path = format!("~/.opendev/plans/{}.md", plan_name);
            let mut state = HashMap::new();
            state.insert("planning_phase".to_string(), serde_json::json!("explore"));
            state.insert("plan_file_path".to_string(), serde_json::json!(plan_path));
            state.insert("explore_count".to_string(), serde_json::json!(0));
            Some(std::sync::Arc::new(std::sync::Mutex::new(state)))
        } else {
            None
        };

        let tool_context = ToolContext {
            working_dir: self.working_dir.clone(),
            is_subagent: false,
            session_id: self.session_manager.current_session().map(|s| s.id.clone()),
            values: HashMap::new(),
            timeout_config: None,
            cancel_token: interrupt_token.map(|t| t.child_token()),
            diagnostic_provider: None,
            shared_state: shared_state.clone(),
        };

        // Inject plan reminder if plan mode is active
        if plan_requested {
            let plan_path = shared_state
                .as_ref()
                .and_then(|s| s.lock().ok())
                .and_then(|s| {
                    s.get("plan_file_path")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                })
                .unwrap_or_default();
            let reminder = opendev_agents::prompts::reminders::get_reminder(
                "plan_subagent_request",
                &[("plan_file_path", &plan_path)],
            );
            if !reminder.is_empty() {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": format!("<system-reminder>{}</system-reminder>", reminder)
                }));
            }
        }

        // Inject existing TODO state reminder if there are incomplete todos
        if let Ok(mgr) = self.todo_manager.lock()
            && mgr.has_todos()
            && mgr.has_incomplete_todos()
        {
            let todo_status = mgr.format_status();
            let reminder = opendev_agents::prompts::reminders::get_reminder(
                "existing_todos_reminder",
                &[("todo_status", &todo_status)],
            );
            if !reminder.is_empty() {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": format!("<system-reminder>{}</system-reminder>", reminder)
                }));
            }
        }

        // Step 6: Set original task for completion nudge context
        self.react_loop.set_original_task(Some(query.to_string()));

        // Step 6b: Snapshot workspace state before the react loop
        let pre_snapshot = if let Ok(mut mgr) = self.snapshot_manager.lock() {
            mgr.track()
        } else {
            None
        };

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
                Some(&*self.debug_logger),
            )
            .await?;

        // For backgrounded results, return immediately without saving messages.
        // The synthetic assistant message is added by the caller (tui_runner) AFTER
        // forking the session, so the background runtime gets a clean session without
        // the synthetic message that could confuse it.
        if result.backgrounded {
            return Ok(result);
        }

        // Step 7b: Snapshot workspace state after the react loop and compute file changes
        if let Some(ref pre_hash) = pre_snapshot
            && let Ok(mut mgr) = self.snapshot_manager.lock()
        {
            let post_hash = mgr.track();
            if let Some(ref post_hash) = post_hash
                && pre_hash != post_hash
            {
                let stats = mgr.diff_numstat(pre_hash, post_hash);
                if !stats.is_empty() {
                    let total_additions: u64 = stats.iter().map(|s| s.additions).sum();
                    let total_deletions: u64 = stats.iter().map(|s| s.deletions).sum();
                    let total_files = stats.len();

                    // Populate session file_changes
                    if let Some(session) = self.session_manager.current_session_mut() {
                        use opendev_models::file_change::{FileChange, FileChangeType};
                        for stat in &stats {
                            let change_type = if stat.additions > 0 && stat.deletions == 0 {
                                // Could be a new file or pure addition
                                FileChangeType::Created
                            } else if stat.additions == 0 && stat.deletions > 0 {
                                FileChangeType::Deleted
                            } else {
                                FileChangeType::Modified
                            };
                            let mut fc = FileChange::new(change_type, stat.file_path.clone());
                            fc.lines_added = stat.additions;
                            fc.lines_removed = stat.deletions;
                            session.add_file_change(fc);
                        }
                    }

                    // Emit file change callback to TUI
                    if let Some(cb) = event_callback {
                        cb.on_file_changed(total_files, total_additions, total_deletions);
                    }

                    info!(
                        files = total_files,
                        additions = total_additions,
                        deletions = total_deletions,
                        "File changes detected after query"
                    );
                }
            }
        }

        // Step 7c: Save all new messages from the react loop to the session.
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

        // Step 9: Auto-detect session title (1st message + every 5th user message)
        if self.topic_detector.is_enabled() {
            let (should_detect, current_title) = self
                .session_manager
                .current_session()
                .map(|s| {
                    let has_title = s.metadata.contains_key("title");
                    let user_msg_count = s.messages.iter().filter(|m| m.role == Role::User).count();
                    let should = !has_title || (user_msg_count > 1 && user_msg_count % 5 == 0);
                    let title = s
                        .metadata
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(|t| t.to_string());
                    (should, title)
                })
                .unwrap_or((false, None));

            if should_detect {
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
                                let truncated = if m.content.len() > 500 {
                                    m.content[..m.content.floor_char_boundary(500)].to_string()
                                } else {
                                    m.content.clone()
                                };
                                Some(SimpleMessage {
                                    role: role.to_string(),
                                    content: truncated,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                if let Some(title) = self
                    .topic_detector
                    .detect_title(&simple_msgs, current_title.as_deref())
                    .await
                {
                    // Skip no-op updates
                    let is_same = current_title.as_deref().is_some_and(|ct| ct == title);

                    if !is_same && let Some(session) = self.session_manager.current_session() {
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

    /// Inject a background agent result as a tool-call/tool-result pair and
    /// run a new react-loop turn so the foreground LLM can synthesize it.
    ///
    /// Instead of injecting the result as a `role: "user"` message (which
    /// makes the LLM treat it as external input), this adds a single
    /// assistant message with a synthetic `get_background_result` tool call
    /// whose result is already populated. `chatmessages_to_api_values` then
    /// emits both the assistant tool_calls and the `role: "tool"` result.
    ///
    /// The LLM sees a natural tool-call → tool-result pair and responds
    /// with a conversational summary rather than re-investigating.
    #[allow(clippy::too_many_arguments)]
    pub async fn inject_background_result(
        &mut self,
        task_id: &str,
        query: &str,
        result: &str,
        tool_call_count: usize,
        system_prompt: &str,
        event_callback: Option<&dyn AgentEventCallback>,
        interrupt_token: Option<&opendev_runtime::InterruptToken>,
    ) -> Result<AgentResult, AgentError> {
        info!(
            task_id,
            tool_call_count, "Injecting background result as tool-result pair"
        );

        // Synthetic tool_call_id linking the assistant call to its result
        let synthetic_id = format!("bg_{task_id}");

        let tool_result_content = format!(
            "[Background task [{task_id}] completed ({tool_call_count} tools)]\n\
             Task: {query}\n\n\
             {result}"
        );

        // Add a single assistant ChatMessage with a tool call whose result is
        // already populated.  `chatmessages_to_api_values` will emit both:
        //   - an assistant message with the tool_calls array
        //   - a role:"tool" message with tool_call_id + content
        let mut tool_call = opendev_models::message::ToolCall {
            id: synthetic_id.clone(),
            name: "get_background_result".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("task_id".to_string(), serde_json::json!(task_id));
                p
            },
            result: Some(serde_json::json!(tool_result_content)),
            result_summary: None,
            timestamp: Utc::now(),
            approved: true,
            error: None,
            nested_tool_calls: Vec::new(),
        };
        if tool_result_content.len() > 300 {
            tool_call.result_summary = Some(format!(
                "Background task {task_id} completed ({tool_call_count} tools) for: {query}"
            ));
        }

        self.session_manager.add_message(ChatMessage {
            role: Role::Assistant,
            content: String::new(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
            tool_calls: vec![tool_call],
            tokens: None,
            thinking_trace: None,
            reasoning_content: None,
            token_usage: None,
            provenance: None,
        });

        // 3. Build messages from session history (includes the new tool pair)
        let session_messages = self
            .session_manager
            .current_session()
            .map(|s| opendev_history::message_convert::chatmessages_to_api_values(&s.messages))
            .unwrap_or_default();

        let mut messages = self.query_enhancer.prepare_messages(
            "", // no user query — the tool result is the trigger
            "",
            system_prompt,
            Some(&session_messages),
            &[],
            false,
            None,
        );

        // 4. Run the react loop
        let tool_schemas = self.tool_registry.get_schemas();
        let tool_context = ToolContext {
            working_dir: self.working_dir.clone(),
            is_subagent: false,
            session_id: self.session_manager.current_session().map(|s| s.id.clone()),
            values: HashMap::new(),
            timeout_config: None,
            cancel_token: interrupt_token.map(|t| t.child_token()),
            diagnostic_provider: None,
            shared_state: None,
        };

        self.react_loop.set_original_task(None);

        let pre_count = messages.len();
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
                Some(&*self.debug_logger),
            )
            .await?;

        // Save new messages from the react loop
        {
            let new_values = &result.messages[pre_count..];
            let new_chat_messages =
                opendev_history::message_convert::api_values_to_chatmessages(new_values);
            for msg in new_chat_messages {
                self.session_manager.add_message(msg);
            }
        }

        if let Err(e) = self.session_manager.save_current() {
            warn!("Failed to save session: {e}");
        }

        // Log session cost
        if let Ok(tracker) = self.cost_tracker.lock() {
            info!(
                cost = tracker.format_cost(),
                calls = tracker.call_count,
                "Session cost update (background result)"
            );
        }

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

        Ok(format!(
            "Conversation compacted: {original_count} messages \u{2192} {compacted_count} messages."
        ))
    }
}
