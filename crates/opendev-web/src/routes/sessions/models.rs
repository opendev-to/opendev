//! Session model overlay management routes.

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use serde::Deserialize;

use crate::error::WebError;
use crate::state::AppState;

/// Session model update request.
#[derive(Debug, Deserialize)]
pub struct SessionModelUpdate {
    pub model_provider: Option<String>,
    pub model: Option<String>,
    pub model_thinking_provider: Option<String>,
    pub model_thinking: Option<String>,
    pub model_vlm_provider: Option<String>,
    pub model_vlm: Option<String>,
    pub model_critique_provider: Option<String>,
    pub model_critique: Option<String>,
    pub model_compact_provider: Option<String>,
    pub model_compact: Option<String>,
}

/// Get session model overlay.
pub(super) async fn get_session_model(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mgr = state.session_manager().await;
    let session = mgr
        .load_session(&id)
        .map_err(|e| WebError::NotFound(format!("Session {} not found: {}", id, e)))?;

    let overlay = session
        .metadata
        .get("session_model")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    Ok(Json(overlay))
}

/// Update session model overlay.
pub(super) async fn update_session_model(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<SessionModelUpdate>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mgr = state.session_manager().await;

    let mut session = mgr
        .load_session(&id)
        .map_err(|e| WebError::NotFound(format!("Session {} not found: {}", id, e)))?;

    // Build overlay from non-None fields.
    let mut overlay = serde_json::Map::new();
    if let Some(v) = body.model_provider {
        overlay.insert("model_provider".to_string(), serde_json::json!(v));
    }
    if let Some(v) = body.model {
        overlay.insert("model".to_string(), serde_json::json!(v));
    }
    if let Some(v) = body.model_thinking_provider {
        overlay.insert("model_thinking_provider".to_string(), serde_json::json!(v));
    }
    if let Some(v) = body.model_thinking {
        overlay.insert("model_thinking".to_string(), serde_json::json!(v));
    }
    if let Some(v) = body.model_vlm_provider {
        overlay.insert("model_vlm_provider".to_string(), serde_json::json!(v));
    }
    if let Some(v) = body.model_vlm {
        overlay.insert("model_vlm".to_string(), serde_json::json!(v));
    }
    if let Some(v) = body.model_critique_provider {
        overlay.insert("model_critique_provider".to_string(), serde_json::json!(v));
    }
    if let Some(v) = body.model_critique {
        overlay.insert("model_critique".to_string(), serde_json::json!(v));
    }
    if let Some(v) = body.model_compact_provider {
        overlay.insert("model_compact_provider".to_string(), serde_json::json!(v));
    }
    if let Some(v) = body.model_compact {
        overlay.insert("model_compact".to_string(), serde_json::json!(v));
    }

    if overlay.is_empty() {
        return Err(WebError::BadRequest("No model fields provided".to_string()));
    }

    // Store overlay in session metadata.
    session.metadata.insert(
        "session_model".to_string(),
        serde_json::Value::Object(overlay),
    );

    mgr.save_session(&session)
        .map_err(|e| WebError::Internal(format!("Failed to save session: {}", e)))?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Session model updated",
    })))
}

/// Clear session model overlay.
pub(super) async fn clear_session_model(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mgr = state.session_manager().await;

    let mut session = mgr
        .load_session(&id)
        .map_err(|e| WebError::NotFound(format!("Session {} not found: {}", id, e)))?;

    session.metadata.remove("session_model");

    mgr.save_session(&session)
        .map_err(|e| WebError::Internal(format!("Failed to save session: {}", e)))?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Session model cleared",
    })))
}
