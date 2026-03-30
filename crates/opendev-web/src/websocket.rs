//! WebSocket handler for real-time communication.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::SinkExt;
use futures::stream::StreamExt;
use tracing::{debug, error, info, warn};

use crate::protocol::WsMessageType;
use crate::state::{AppState, WsBroadcast};

/// WebSocket upgrade handler.
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle an individual WebSocket connection.
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel.
    let mut broadcast_rx = state.ws_subscribe();

    // Spawn task to forward broadcasts to this client.
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = broadcast_rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(text) => {
                    if sender.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize broadcast message: {}", e);
                }
            }
        }
    });

    // Receive messages from this client.
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                handle_client_message(&state, &text).await;
            }
            Message::Close(_) => {
                info!("WebSocket client disconnected");
                break;
            }
            _ => {}
        }
    }

    // Clean up the send task.
    send_task.abort();
    info!("WebSocket connection closed");
}

/// Handle a text message from a WebSocket client.
async fn handle_client_message(state: &AppState, text: &str) {
    let parsed: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            warn!("Invalid WebSocket message JSON: {}", e);
            return;
        }
    };

    let msg_type_str = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");

    let msg_type = WsMessageType::from_str_opt(msg_type_str);

    match msg_type {
        Some(WsMessageType::Ping) => {
            state.broadcast(WsBroadcast::new(
                WsMessageType::Pong.as_str().to_string(),
                serde_json::Value::Null,
            ));
        }
        Some(WsMessageType::Query) => {
            handle_query(state, &parsed).await;
        }
        Some(WsMessageType::Approve) => {
            handle_approval(state, &parsed).await;
        }
        Some(WsMessageType::AskUserResponse) => {
            handle_ask_user_response(state, &parsed).await;
        }
        Some(WsMessageType::PlanApprovalResponse) => {
            handle_plan_approval_response(state, &parsed).await;
        }
        Some(WsMessageType::Interrupt) => {
            handle_interrupt(state).await;
        }
        _ => {
            if !msg_type_str.is_empty() {
                warn!("Unknown WebSocket message type: {}", msg_type_str);
            }
            state.broadcast(WsBroadcast::new(
                WsMessageType::Error.as_str().to_string(),
                serde_json::json!({
                "message": format!("Unknown message type: {}", msg_type_str),
                }),
            ));
        }
    }
}

/// Handle a query message from a WebSocket client.
async fn handle_query(state: &AppState, data: &serde_json::Value) {
    let message = data
        .get("data")
        .and_then(|d| d.get("message"))
        .and_then(|m| m.as_str());
    let session_id = data
        .get("data")
        .and_then(|d| d.get("session_id"))
        .and_then(|s| s.as_str());

    let message = match message {
        Some(m) if !m.trim().is_empty() => m.trim(),
        _ => {
            state.broadcast(WsBroadcast::new(
                WsMessageType::Error.as_str().to_string(),
                serde_json::json!({"message": "Missing or empty message field"}),
            ));
            return;
        }
    };

    // Resolve session ID: use provided or fall back to current.
    let session_id = match session_id {
        Some(id) => id.to_string(),
        None => match state.current_session_id().await {
            Some(id) => id,
            None => {
                state.broadcast(WsBroadcast::new(
                    WsMessageType::Error.as_str().to_string(),
                    serde_json::json!({"message": "No active session"}),
                ));
                return;
            }
        },
    };

    // Bridge mode: route to TUI injector instead of agent executor.
    if state.is_bridge_guarded(&session_id).await {
        // In bridge mode, broadcast the user message then inject into the
        // TUI's queue (same injection mechanism used for live messages).
        state.broadcast(WsBroadcast::new(
            WsMessageType::UserMessage.as_str().to_string(),
            serde_json::json!({
            "role": "user",
            "content": message,
            "session_id": session_id,
            }),
        ));

        match state
            .try_inject_message(&session_id, message.to_string())
            .await
        {
            Ok(()) => {}
            Err(e) => {
                state.broadcast(WsBroadcast::new(
                    WsMessageType::Error.as_str().to_string(),
                    serde_json::json!({
                    "message": format!("Bridge mode injection failed: {}", e),
                    }),
                ));
            }
        }
        return;
    }

    // If session is already running, inject into live queue.
    if state.is_session_running(&session_id).await {
        match state
            .try_inject_message(&session_id, message.to_string())
            .await
        {
            Ok(()) => {
                state.broadcast(WsBroadcast::new(
                    WsMessageType::UserMessage.as_str().to_string(),
                    serde_json::json!({
                    "role": "user",
                    "content": message,
                    "session_id": session_id,
                    "injected": true,
                    }),
                ));
            }
            Err(e) => {
                state.broadcast(WsBroadcast::new(
                    WsMessageType::Error.as_str().to_string(),
                    serde_json::json!({
                    "message": e,
                    "session_id": session_id,
                    }),
                ));
            }
        }
        return;
    }

    // Broadcast user message.
    state.broadcast(WsBroadcast::new(
        WsMessageType::UserMessage.as_str().to_string(),
        serde_json::json!({
        "role": "user",
        "content": message,
        "session_id": session_id,
        }),
    ));

    // Fire the agent executor in the background (if set).
    if let Some(executor) = state.agent_executor().await {
        let state_clone = state.clone();
        let message_owned = message.to_string();
        let session_id_owned = session_id.clone();
        tokio::spawn(async move {
            if let Err(e) = executor
                .execute_query(message_owned, session_id_owned, state_clone)
                .await
            {
                error!("Agent executor error: {}", e);
            }
        });
    } else {
        debug!(
            "Query received for session {} but no agent executor is set: {}",
            session_id, message
        );
    }
}

/// Handle an approval response from a WebSocket client.
async fn handle_approval(state: &AppState, data: &serde_json::Value) {
    let approval_data = data.get("data").cloned().unwrap_or_default();
    let approval_id = approval_data
        .get("approvalId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let approved = approval_data
        .get("approved")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let auto_approve = approval_data
        .get("autoApprove")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if approval_id.is_empty() {
        state.broadcast(WsBroadcast::new(
            WsMessageType::Error.as_str().to_string(),
            serde_json::json!({"message": "Invalid approval data"}),
        ));
        return;
    }

    let resolved = state
        .resolve_approval(approval_id, approved, auto_approve)
        .await;

    if let Some(approval) = resolved {
        info!("Approval {} resolved: approved={}", approval_id, approved);
        state.broadcast(WsBroadcast::new(
            WsMessageType::ApprovalResolved.as_str().to_string(),
            serde_json::json!({
            "approvalId": approval_id,
            "approved": approved,
            "session_id": approval.session_id,
            }),
        ));
    } else {
        warn!("Approval {} not found", approval_id);
    }
}

/// Handle an ask-user response from a WebSocket client.
async fn handle_ask_user_response(state: &AppState, data: &serde_json::Value) {
    let response_data = data.get("data").cloned().unwrap_or_default();
    let request_id = response_data
        .get("requestId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let answers = response_data.get("answers").cloned();
    let cancelled = response_data
        .get("cancelled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if request_id.is_empty() {
        state.broadcast(WsBroadcast::new(
            WsMessageType::Error.as_str().to_string(),
            serde_json::json!({"message": "Invalid ask-user response data"}),
        ));
        return;
    }

    let resolved = state.resolve_ask_user(request_id, answers, cancelled).await;

    if let Some(ask_user) = resolved {
        info!("Ask-user {} resolved", request_id);
        state.broadcast(WsBroadcast::new(
            WsMessageType::AskUserResolved.as_str().to_string(),
            serde_json::json!({
            "requestId": request_id,
            "session_id": ask_user.session_id,
            }),
        ));
    } else {
        warn!("Ask-user request {} not found", request_id);
    }
}

/// Handle a plan approval response from a WebSocket client.
async fn handle_plan_approval_response(state: &AppState, data: &serde_json::Value) {
    let response_data = data.get("data").cloned().unwrap_or_default();
    let request_id = response_data
        .get("requestId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let action = response_data
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("reject")
        .to_string();
    let feedback = response_data
        .get("feedback")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if request_id.is_empty() {
        state.broadcast(WsBroadcast::new(
            WsMessageType::Error.as_str().to_string(),
            serde_json::json!({"message": "Invalid plan approval response data"}),
        ));
        return;
    }

    let resolved = state
        .resolve_plan_approval(request_id, action.clone(), feedback)
        .await;

    if let Some(plan_approval) = resolved {
        info!("Plan approval {} resolved: action={}", request_id, action);
        state.broadcast(WsBroadcast::new(
            WsMessageType::PlanApprovalResolved.as_str().to_string(),
            serde_json::json!({
            "requestId": request_id,
            "action": action,
            "session_id": plan_approval.session_id,
            }),
        ));
    } else {
        warn!("Plan approval request {} not found", request_id);
    }
}

/// Handle an interrupt request from a WebSocket client.
async fn handle_interrupt(state: &AppState) {
    info!("Interrupt requested via WebSocket");
    state.request_interrupt().await;

    state.broadcast(WsBroadcast::new(
        WsMessageType::StatusUpdate.as_str().to_string(),
        serde_json::json!({
        "interrupted": true,
        }),
    ));
}
