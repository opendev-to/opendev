//! Chat message routes.

use axum::extract::State;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tracing::{error, info};

use crate::error::WebError;
use crate::state::{AppState, WsBroadcast};

/// Chat query request.
#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub message: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Interrupt request.
#[derive(Debug, Deserialize)]
pub struct InterruptRequest {
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Clear chat request.
#[derive(Debug, Deserialize)]
pub struct ClearChatRequest {
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Build the chat router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/chat/messages", get(get_messages))
        .route("/api/chat/query", post(send_query))
        .route("/api/chat/interrupt", post(interrupt))
        .route("/api/chat/clear", delete(clear_chat))
}

/// Get messages for the current session.
async fn get_messages(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let mgr = state.session_manager().await;
    let session = mgr
        .current_session()
        .ok_or_else(|| WebError::NotFound("No active session".to_string()))?;

    let messages: Vec<serde_json::Value> = session
        .messages
        .iter()
        .filter(|msg| {
            // Skip system-injected messages (nudges, directives, internal)
            if msg.metadata.contains_key("_msg_class") {
                return false;
            }
            // Skip [SYSTEM] prefixed messages from older sessions
            if msg.role == opendev_models::message::Role::User
                && msg.content.starts_with("[SYSTEM] ")
            {
                return false;
            }
            // Skip system messages
            if msg.role == opendev_models::message::Role::System {
                return false;
            }
            true
        })
        .map(|msg| {
            let mut val = serde_json::json!({
                "role": msg.role,
                "content": msg.content,
                "timestamp": msg.timestamp,
                "tool_calls": msg.tool_calls.iter()
                    .filter(|tc| tc.name != "task_complete")
                    .count(),
            });
            if let Some(ref reasoning) = msg.reasoning_content {
                val["reasoning_content"] = serde_json::json!(reasoning);
            }
            if let Some(ref trace) = msg.thinking_trace {
                val["thinking_trace"] = serde_json::json!(trace);
            }
            val
        })
        .collect();

    Ok(Json(serde_json::json!(messages)))
}

/// Send a query to the agent.
///
/// 4-case dispatch:
/// 1. Empty message -> 400 Bad Request
/// 2. Session already running -> inject into live queue; 409 if full
/// 3. Normal -> load session, persist message, broadcast, fire agent loop
/// 4. No agent executor set -> accept but warn
async fn send_query(
    State(state): State<AppState>,
    Json(payload): Json<QueryRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    // Case 1: Empty message.
    let message = payload.message.trim().to_string();
    if message.is_empty() {
        return Err(WebError::BadRequest("Message cannot be empty.".to_string()));
    }

    // Resolve session ID.
    let session_id = match payload.session_id {
        Some(id) => id,
        None => state.current_session_id().await.ok_or_else(|| {
            WebError::BadRequest("No active session. Create a session first.".to_string())
        })?,
    };

    // Case 2: Session already running -> inject into live queue.
    if state.is_session_running(&session_id).await {
        match state.try_inject_message(&session_id, message.clone()).await {
            Ok(()) => {
                // Broadcast the injected user message.
                state.broadcast(WsBroadcast::new(
                    "user_message".to_string(),
                    serde_json::json!({
                    "role": "user",
                    "content": message,
                    "session_id": session_id,
                    "injected": true,
                    }),
                ));
                return Ok(Json(serde_json::json!({
                    "status": "accepted",
                    "session_id": session_id,
                })));
            }
            Err(_) => {
                return Err(WebError::Conflict(
                    "Agent is busy; injection queue is full. Try again shortly.".to_string(),
                ));
            }
        }
    }

    // Case 3: Normal flow — load session, persist message, broadcast, fire agent.

    // Load session (try from session manager, fall back to current).
    let mgr = state.session_manager().await;
    let session_exists = mgr.load_session(&session_id).is_ok()
        || mgr
            .current_session()
            .map(|s| s.id == session_id)
            .unwrap_or(false);
    drop(mgr);

    if !session_exists {
        return Err(WebError::NotFound(format!(
            "Session '{}' not found.",
            session_id
        )));
    }

    // Broadcast user message to WebSocket clients.
    state.broadcast(WsBroadcast::new(
        "user_message".to_string(),
        serde_json::json!({
        "role": "user",
        "content": message,
        "session_id": session_id,
        }),
    ));

    // Fire the agent executor as a background task.
    if let Some(executor) = state.agent_executor().await {
        let state_clone = state.clone();
        let msg = message.clone();
        let sid = session_id.clone();
        tokio::spawn(async move {
            if let Err(e) = executor.execute_query(msg, sid, state_clone).await {
                error!("Agent executor error: {}", e);
            }
        });
    } else {
        info!(
            "Query accepted for session {} but no agent executor is wired",
            session_id
        );
    }

    Ok(Json(serde_json::json!({
        "status": "accepted",
        "session_id": session_id,
    })))
}

/// Interrupt an ongoing task.
///
/// Calls `request_interrupt()` which also denies all pending approvals and
/// ask-user requests via their oneshot channels.
async fn interrupt(
    State(state): State<AppState>,
    Json(_payload): Json<InterruptRequest>,
) -> Json<serde_json::Value> {
    state.request_interrupt().await;

    state.broadcast(WsBroadcast::new(
        "interrupt".to_string(),
        serde_json::json!({"status": "requested"}),
    ));

    Json(serde_json::json!({
        "status": "interrupt_requested",
    }))
}

/// Clear the current chat session by creating a new one.
async fn clear_chat(
    State(state): State<AppState>,
    body: Option<Json<ClearChatRequest>>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mut mgr = state.session_manager_mut().await;
    let session = mgr.create_session();
    let session_id = session.id.clone();

    // Set working directory if provided.
    if let Some(Json(req)) = body
        && let Some(wd) = req.workspace
        && let Some(session) = mgr.current_session_mut()
    {
        session.working_directory = Some(wd);
    }

    mgr.save_current()
        .map_err(|e| WebError::Internal(format!("Failed to save new session: {}", e)))?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Chat cleared",
        "session_id": session_id,
    })))
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
