//! Session management routes.

mod filesystem;
mod models;

use axum::extract::{Path as AxumPath, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::WebError;
use crate::state::AppState;

pub use filesystem::{BrowseDirectoryRequest, ListFilesQuery, VerifyPathRequest};
pub use models::SessionModelUpdate;

/// Create session request.
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub working_directory: Option<String>,
}

/// Build the sessions router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/bridge-info", get(get_bridge_info))
        .route("/api/sessions/files", get(filesystem::list_files))
        .route("/api/sessions/verify-path", post(filesystem::verify_path))
        .route(
            "/api/sessions/browse-directory",
            post(filesystem::browse_directory),
        )
        .route(
            "/api/sessions/{id}",
            get(get_session).delete(delete_session),
        )
        .route("/api/sessions/{id}/resume", post(resume_session))
        .route("/api/sessions/{id}/messages", get(get_session_messages))
        .route(
            "/api/sessions/{id}/model",
            get(models::get_session_model)
                .put(models::update_session_model)
                .delete(models::clear_session_model),
        )
}

/// List all sessions.
async fn list_sessions(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let mgr = state.session_manager().await;
    let index = mgr.index().read_index();

    let sessions: Vec<serde_json::Value> = match index {
        Some(idx) => idx
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "id": entry.session_id,
                    "created_at": entry.created,
                    "updated_at": entry.modified,
                    "message_count": entry.message_count,
                    "title": entry.title,
                    "working_directory": entry.working_directory,
                })
            })
            .collect(),
        None => Vec::new(),
    };

    Ok(Json(serde_json::json!(sessions)))
}

/// Create a new session.
///
/// Before creating a brand-new session, checks if there is an existing empty
/// session (message_count == 0) for the same workspace. If found and not stale,
/// the existing session is reused instead of creating a new one.
async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateSessionRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let requested_wd = payload.working_directory.clone();

    let mut mgr = state.session_manager_mut().await;

    // Try to reuse an existing empty session for the same workspace.
    if let Some(ref wd) = requested_wd
        && let Some(index) = mgr.index().read_index()
    {
        let empty_match = index.entries.iter().find(|entry| {
            entry.message_count == 0
                && entry
                    .working_directory
                    .as_deref()
                    .map(|d| d == wd.as_str())
                    .unwrap_or(false)
        });

        if let Some(entry) = empty_match {
            let candidate_id = entry.session_id.clone();

            // Guard against stale index: if the candidate is already the
            // current session with in-memory messages, skip reuse.
            let is_stale = mgr
                .current_session()
                .map(|s| s.id == candidate_id && !s.messages.is_empty())
                .unwrap_or(false);

            if !is_stale {
                // Try to load and resume the candidate session.
                if mgr.resume_session(&candidate_id).is_ok() {
                    return Ok(Json(serde_json::json!({
                        "id": candidate_id,
                        "status": "reused",
                        "message": "Reusing existing empty session",
                    })));
                }
                // If load fails (e.g. file deleted), fall through to create new.
            }
        }
    }

    // No reusable session found — create a new one.
    let session = mgr.create_session();
    let session_id = session.id.clone();

    // Set working directory if provided.
    if let Some(wd) = requested_wd
        && let Some(session) = mgr.current_session_mut()
    {
        session.working_directory = Some(wd);
    }

    // Save the new session.
    mgr.save_current()
        .map_err(|e| WebError::Internal(format!("Failed to save session: {}", e)))?;

    Ok(Json(serde_json::json!({
        "id": session_id,
        "status": "created",
    })))
}

/// Get a specific session.
async fn get_session(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mgr = state.session_manager().await;
    let session = mgr
        .load_session(&id)
        .map_err(|e| WebError::NotFound(format!("Session {} not found: {}", id, e)))?;

    Ok(Json(serde_json::to_value(session.get_metadata()).map_err(
        |e| WebError::Internal(format!("Failed to serialize session: {}", e)),
    )?))
}

/// Delete a specific session.
async fn delete_session(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mut mgr = state.session_manager_mut().await;

    // Check session exists.
    mgr.load_session(&id)
        .map_err(|e| WebError::NotFound(format!("Session {} not found: {}", id, e)))?;

    // Delete session files (.json, .jsonl).
    let session_dir = mgr.session_dir().to_path_buf();
    let json_path = session_dir.join(format!("{}.json", id));
    let jsonl_path = session_dir.join(format!("{}.jsonl", id));
    let debug_path = session_dir.join(format!("{}.debug", id));

    if json_path.exists() {
        std::fs::remove_file(&json_path)
            .map_err(|e| WebError::Internal(format!("Failed to delete session file: {}", e)))?;
    }
    if jsonl_path.exists() {
        std::fs::remove_file(&jsonl_path).map_err(|e| {
            WebError::Internal(format!("Failed to delete session transcript: {}", e))
        })?;
    }
    if debug_path.exists() {
        let _ = std::fs::remove_file(&debug_path);
    }

    // Remove from index.
    mgr.index()
        .remove_entry(&id)
        .map_err(|e| WebError::Internal(format!("Failed to update index: {}", e)))?;

    // Clear current session if it was the deleted one.
    if mgr.current_session().map(|s| s.id == id).unwrap_or(false) {
        mgr.set_current_session(opendev_models::Session::new());
    }

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": format!("Session {} deleted", id),
    })))
}

/// Resume a session.
async fn resume_session(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mut mgr = state.session_manager_mut().await;
    mgr.resume_session(&id)
        .map_err(|e| WebError::NotFound(format!("Session {} not found: {}", id, e)))?;

    Ok(Json(serde_json::json!({
        "status": "resumed",
        "session_id": id,
    })))
}

/// Get messages for a session.
async fn get_session_messages(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mgr = state.session_manager().await;
    let session = mgr
        .load_session(&id)
        .map_err(|e| WebError::NotFound(format!("Session {} not found: {}", id, e)))?;

    let messages: Vec<serde_json::Value> = session
        .messages
        .iter()
        .map(|msg| {
            let tool_calls: Vec<serde_json::Value> = msg
                .tool_calls
                .iter()
                .map(|tc| {
                    let mut val = serde_json::json!({
                        "id": tc.id,
                        "name": tc.name,
                        "parameters": tc.parameters,
                    });
                    if let Some(ref result) = tc.result {
                        val["result"] = serde_json::json!(result);
                    }
                    if let Some(ref summary) = tc.result_summary {
                        val["result_summary"] = serde_json::json!(summary);
                    }
                    if let Some(ref error) = tc.error {
                        val["error"] = serde_json::json!(error);
                    }
                    if !tc.nested_tool_calls.is_empty() {
                        val["nested_tool_calls"] = serde_json::json!(tc.nested_tool_calls);
                    }
                    val
                })
                .collect();

            let mut val = serde_json::json!({
                "role": msg.role,
                "content": msg.content,
                "timestamp": msg.timestamp,
            });
            if !tool_calls.is_empty() {
                val["tool_calls"] = serde_json::json!(tool_calls);
            }
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

/// Get bridge mode status.
async fn get_bridge_info(State(_state): State<AppState>) -> Json<serde_json::Value> {
    // Bridge mode is not yet implemented in the Rust port.
    // Return a default non-bridge response.
    Json(serde_json::json!({
        "bridge_mode": false,
        "session_id": null,
    }))
}

#[cfg(test)]
mod tests;
