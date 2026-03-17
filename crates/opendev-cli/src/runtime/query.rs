//! Query execution pipeline, MCP integration, and context compaction.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tracing::{debug, info, warn};

use opendev_agents::prompts::create_thinking_composer;
use opendev_agents::traits::{AgentError, AgentEventCallback, AgentResult};
use opendev_context::ContextCompactor;
use opendev_history::topic_detector::SimpleMessage;
use opendev_mcp::McpManager;
use opendev_models::message::{ChatMessage, Role};
use opendev_tools_core::ToolContext;
use opendev_tools_impl::*;

use super::AgentRuntime;

impl AgentRuntime {
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
            )
            .await?;

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
