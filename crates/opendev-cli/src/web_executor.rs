//! WebAgentExecutor: implements the AgentExecutor trait for the web backend.
//!
//! This bridges agent execution events to WebSocket broadcasts so the Web UI
//! receives the same real-time feedback as the TUI.

use std::collections::HashMap;
use std::sync::Arc;

use opendev_agents::AgentEventCallback;
use opendev_web::state::{AppState, WsBroadcast};
use uuid::Uuid;

/// Callback that broadcasts agent events over WebSocket.
pub struct WebEventCallback {
    state: AppState,
    session_id: String,
    todo_manager: Option<std::sync::Arc<std::sync::Mutex<opendev_runtime::TodoManager>>>,
}

impl WebEventCallback {
    pub fn new(
        state: AppState,
        session_id: String,
        todo_manager: Option<std::sync::Arc<std::sync::Mutex<opendev_runtime::TodoManager>>>,
    ) -> Self {
        Self {
            state,
            session_id,
            todo_manager,
        }
    }

    fn broadcast(&self, msg_type: &str, data: serde_json::Value) {
        self.state.broadcast(WsBroadcast::new(msg_type, data));
    }
}

impl AgentEventCallback for WebEventCallback {
    fn on_tool_started(
        &self,
        tool_id: &str,
        tool_name: &str,
        args: &HashMap<String, serde_json::Value>,
    ) {
        self.broadcast(
            "tool_call",
            serde_json::json!({
                "session_id": self.session_id,
                "tool_id": tool_id,
                "tool_name": tool_name,
                "arguments": args,
            }),
        );
    }

    fn on_tool_finished(&self, tool_id: &str, success: bool) {
        // tool_finished is implicit in tool_result — no separate WS event needed
        let _ = (tool_id, success);
    }

    fn on_tool_result(&self, tool_id: &str, tool_name: &str, output: &str, success: bool) {
        let mut data = serde_json::json!({
            "session_id": self.session_id,
            "tool_id": tool_id,
            "tool_name": tool_name,
            "output": output,
            "success": success,
        });

        // For todo tools, include current todo state so frontend can update the panel
        let is_todo_tool = matches!(
            tool_name,
            "write_todos" | "update_todo" | "complete_todo" | "clear_todos" | "list_todos"
        );
        if is_todo_tool
            && let Some(tm) = &self.todo_manager
            && let Ok(mgr) = tm.lock()
        {
            let todos: Vec<serde_json::Value> = mgr
                .all()
                .iter()
                .map(|item| {
                    let status = match item.status {
                        opendev_runtime::TodoStatus::Pending => "pending",
                        opendev_runtime::TodoStatus::InProgress => "in_progress",
                        opendev_runtime::TodoStatus::Completed => "completed",
                    };
                    let children: Vec<serde_json::Value> = item
                        .children
                        .iter()
                        .map(|c| serde_json::json!({"title": c.title, "status": "pending"}))
                        .collect();
                    serde_json::json!({
                        "id": item.id.to_string(),
                        "title": item.title,
                        "content": item.title,
                        "status": status,
                        "active_form": item.active_form,
                        "children": children,
                    })
                })
                .collect();
            data["todos"] = serde_json::Value::Array(todos);
        }

        self.broadcast("tool_result", data);
    }

    fn on_agent_chunk(&self, text: &str) {
        self.broadcast(
            "message_chunk",
            serde_json::json!({
                "session_id": self.session_id,
                "content": text,
            }),
        );
    }

    fn on_reasoning(&self, content: &str) {
        self.broadcast(
            "thinking_block",
            serde_json::json!({
                "session_id": self.session_id,
                "content": content,
            }),
        );
    }

    fn on_reasoning_block_start(&self) {
        self.broadcast(
            "thinking_block",
            serde_json::json!({
                "session_id": self.session_id,
                "content": "",
                "block_start": true,
            }),
        );
    }

    fn on_context_usage(&self, pct: f64) {
        self.broadcast(
            "status_update",
            serde_json::json!({
                "session_id": self.session_id,
                "context_usage_pct": pct,
            }),
        );
    }

    fn on_token_usage(&self, input_tokens: u64, output_tokens: u64) {
        self.broadcast(
            "status_update",
            serde_json::json!({
                "session_id": self.session_id,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
            }),
        );
    }

    fn on_file_changed(&self, files: usize, additions: u64, deletions: u64) {
        self.broadcast(
            "status_update",
            serde_json::json!({
                "session_id": self.session_id,
                "file_changes": {
                    "files": files,
                    "additions": additions,
                    "deletions": deletions,
                },
            }),
        );
    }
}

/// Spawn bridge tasks that forward channel events to WebSocket broadcasts.
/// Must be called once after taking `channel_receivers` from the `AgentRuntime`.
pub fn spawn_channel_bridges(receivers: crate::runtime::ToolChannelReceivers, state: AppState) {
    // Bridge subagent events
    if let Some(mut subagent_rx) = receivers.subagent_event_rx {
        let st = state.clone();
        tokio::spawn(async move {
            use opendev_tools_impl::SubagentEvent;
            while let Some(evt) = subagent_rx.recv().await {
                let (msg_type, data) = match evt {
                    SubagentEvent::Started {
                        subagent_id,
                        subagent_name,
                        task,
                        ..
                    } => (
                        "subagent_start",
                        serde_json::json!({
                            "subagent_id": subagent_id,
                            "agent_type": subagent_name,
                            "description": task,
                        }),
                    ),
                    SubagentEvent::ToolCall {
                        subagent_id,
                        subagent_name: _,
                        tool_name,
                        tool_id,
                        args,
                    } => (
                        "nested_tool_call",
                        serde_json::json!({
                            "subagent_id": subagent_id,
                            "parent_subagent_id": subagent_id,
                            "tool_name": tool_name,
                            "tool_id": tool_id,
                            "tool_call_id": tool_id,
                            "arguments": args,
                            "depth": 1,
                        }),
                    ),
                    SubagentEvent::ToolComplete {
                        subagent_id,
                        subagent_name: _,
                        tool_name,
                        tool_id,
                        success,
                    } => (
                        "nested_tool_result",
                        serde_json::json!({
                            "subagent_id": subagent_id,
                            "parent_subagent_id": subagent_id,
                            "tool_name": tool_name,
                            "tool_id": tool_id,
                            "tool_call_id": tool_id,
                            "success": success,
                            "depth": 1,
                        }),
                    ),
                    SubagentEvent::Finished {
                        subagent_id,
                        subagent_name: _,
                        success,
                        result_summary,
                        tool_call_count,
                        shallow_warning,
                    } => (
                        "subagent_complete",
                        serde_json::json!({
                            "subagent_id": subagent_id,
                            "tool_call_id": subagent_id,
                            "success": success,
                            "result_summary": result_summary,
                            "summary": result_summary,
                            "tool_call_count": tool_call_count,
                            "shallow_warning": shallow_warning,
                        }),
                    ),
                    SubagentEvent::TokenUpdate {
                        subagent_id,
                        subagent_name: _,
                        input_tokens,
                        output_tokens,
                    } => (
                        "status_update",
                        serde_json::json!({
                            "subagent_id": subagent_id,
                            "token_count": input_tokens + output_tokens,
                        }),
                    ),
                };
                st.broadcast(WsBroadcast::new(msg_type, data));
            }
        });
    }

    // Bridge ask-user channel
    {
        let st = state.clone();
        let mut ask_rx = receivers.ask_user_rx;
        tokio::spawn(async move {
            while let Some(req) = ask_rx.recv().await {
                let request_id = Uuid::new_v4().to_string();
                st.broadcast(WsBroadcast::new(
                    "ask_user_required",
                    serde_json::json!({
                        "request_id": request_id,
                        "question": req.question,
                        "options": req.options,
                        "default": req.default,
                    }),
                ));

                // Register pending ask-user so WebSocket response can resolve it
                let rx = st
                    .add_pending_ask_user(
                        request_id,
                        opendev_web::state::PendingAskUser {
                            prompt: req.question.clone(),
                            session_id: None,
                        },
                    )
                    .await;

                // Wait for resolution from WebSocket
                match rx.await {
                    Ok(result) => {
                        let answer = if result.cancelled {
                            String::new()
                        } else {
                            result
                                .answers
                                .and_then(|v| v.as_str().map(String::from))
                                .unwrap_or_default()
                        };
                        let _ = req.response_tx.send(answer);
                    }
                    Err(_) => {
                        let _ = req.response_tx.send(String::new());
                    }
                }
            }
        });
    }

    // Bridge tool approval channel
    {
        let st = state.clone();
        let mut tool_rx = receivers.tool_approval_rx;
        tokio::spawn(async move {
            while let Some(req) = tool_rx.recv().await {
                let approval_id = Uuid::new_v4().to_string();
                st.broadcast(WsBroadcast::new(
                    "approval_required",
                    serde_json::json!({
                        "id": approval_id,
                        "tool_name": "bash",
                        "command": req.command,
                        "working_dir": req.working_dir,
                        "description": format!("Run: {}", req.command),
                    }),
                ));

                let rx = st
                    .add_pending_approval(
                        approval_id,
                        opendev_web::state::PendingApproval {
                            tool_name: "bash".to_string(),
                            arguments: serde_json::json!({"command": req.command}),
                            session_id: None,
                        },
                    )
                    .await;

                match rx.await {
                    Ok(result) => {
                        let choice = if result.approved {
                            if result.auto_approve {
                                "yes_remember"
                            } else {
                                "yes"
                            }
                        } else {
                            "no"
                        };
                        let _ = req.response_tx.send(opendev_runtime::ToolApprovalDecision {
                            approved: result.approved,
                            choice: choice.to_string(),
                            command: req.command.clone(),
                        });
                    }
                    Err(_) => {
                        let _ = req.response_tx.send(opendev_runtime::ToolApprovalDecision {
                            approved: false,
                            choice: "no".to_string(),
                            command: req.command.clone(),
                        });
                    }
                }
            }
        });
    }

    // Bridge plan approval channel
    {
        let st = state;
        let mut plan_rx = receivers.plan_approval_rx;
        tokio::spawn(async move {
            while let Some(req) = plan_rx.recv().await {
                let request_id = Uuid::new_v4().to_string();
                st.broadcast(WsBroadcast::new(
                    "plan_approval_required",
                    serde_json::json!({
                        "request_id": request_id,
                        "plan_content": req.plan_content,
                    }),
                ));

                let rx = st
                    .add_pending_plan_approval(
                        request_id,
                        opendev_web::state::PendingPlanApproval {
                            data: serde_json::json!({"plan_content": req.plan_content}),
                            session_id: None,
                        },
                    )
                    .await;

                match rx.await {
                    Ok(result) => {
                        let _ = req.response_tx.send(opendev_runtime::PlanDecision {
                            action: result.action,
                            feedback: result.feedback,
                        });
                    }
                    Err(_) => {
                        let _ = req.response_tx.send(opendev_runtime::PlanDecision {
                            action: "reject".to_string(),
                            feedback: String::new(),
                        });
                    }
                }
            }
        });
    }
}

/// Agent executor that runs queries through the AgentRuntime and broadcasts events.
pub struct WebAgentExecutor {
    runtime: Arc<tokio::sync::Mutex<crate::runtime::AgentRuntime>>,
    system_prompt: String,
}

impl WebAgentExecutor {
    pub fn new(runtime: crate::runtime::AgentRuntime, system_prompt: String) -> Self {
        Self {
            runtime: Arc::new(tokio::sync::Mutex::new(runtime)),
            system_prompt,
        }
    }
}

#[async_trait::async_trait]
impl opendev_web::state::AgentExecutor for WebAgentExecutor {
    async fn execute_query(
        &self,
        message: String,
        session_id: String,
        state: AppState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let todo_manager = {
            let rt = self.runtime.lock().await;
            Some(std::sync::Arc::clone(&rt.todo_manager))
        };
        let callback = WebEventCallback::new(state.clone(), session_id.clone(), todo_manager);

        // Ensure the runtime's session manager has the web session loaded.
        // If the session exists on disk (previously saved), resume it.
        // Otherwise create a new session with the same ID so messages
        // are saved under the web session's ID.
        {
            let mut rt = self.runtime.lock().await;
            let needs_load = rt
                .session_manager
                .current_session()
                .is_none_or(|s| s.id != session_id);
            if needs_load && rt.session_manager.resume_session(&session_id).is_err() {
                // Session not on disk yet — create one with matching ID
                let mut session = opendev_models::Session::new();
                session.id = session_id.clone();
                session.working_directory = Some(state.working_dir().to_string());
                rt.session_manager.set_current_session(session);
            }
        }

        // Broadcast message_start
        state.broadcast(WsBroadcast::new(
            "message_start",
            serde_json::json!({ "session_id": session_id }),
        ));

        // Mark session as running
        state.set_session_running(session_id.clone()).await;

        // Broadcast session activity
        state.broadcast(WsBroadcast::new(
            "session_activity",
            serde_json::json!({
                "session_id": session_id,
                "running": true,
            }),
        ));

        // Create interrupt token
        let interrupt_token = opendev_runtime::InterruptToken::new();

        // Determine if plan mode
        let plan_requested = state.mode().await == opendev_web::state::OperationMode::Plan;

        // Run the query
        let result = {
            let mut rt = self.runtime.lock().await;
            rt.run_query(
                &message,
                &self.system_prompt,
                Some(&callback),
                Some(&interrupt_token),
                plan_requested,
            )
            .await
        };

        // Broadcast message_complete
        match &result {
            Ok(query_result) => {
                if !query_result.content.is_empty() {
                    state.broadcast(WsBroadcast::new(
                        "message_complete",
                        serde_json::json!({
                            "session_id": session_id,
                            "message": {
                                "role": "assistant",
                                "content": query_result.content,
                            },
                        }),
                    ));
                }
            }
            Err(e) => {
                state.broadcast(WsBroadcast::new(
                    "error",
                    serde_json::json!({
                        "session_id": session_id,
                        "message": e.to_string(),
                    }),
                ));
            }
        }

        // Save session to disk and reload into web's session manager
        {
            let rt = self.runtime.lock().await;
            if let Err(e) = rt.session_manager.save_current() {
                tracing::warn!("Failed to save session after query: {e}");
            }
        }

        // Reload the session from disk into the web's session manager
        // so the messages API returns the updated conversation
        {
            let mut web_mgr = state.session_manager_mut().await;
            if let Err(e) = web_mgr.resume_session(&session_id) {
                tracing::warn!("Failed to reload session into web manager: {e}");
            }
        }

        // Mark session idle
        state.set_session_idle(&session_id).await;

        // Broadcast session activity
        state.broadcast(WsBroadcast::new(
            "session_activity",
            serde_json::json!({
                "session_id": session_id,
                "running": false,
            }),
        ));

        result
            .map(|_| ())
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}
