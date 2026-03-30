//! Configuration routes.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::WebError;
use crate::protocol::WsMessageType;
use crate::state::{AppState, OperationMode, WsBroadcast};

/// Configuration update request.
#[derive(Debug, Deserialize)]
pub struct ConfigUpdate {
    pub model_provider: Option<String>,
    pub model: Option<String>,
    pub model_vlm_provider: Option<String>,
    pub model_vlm: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub enable_bash: Option<bool>,
}

/// Mode update request.
#[derive(Debug, Deserialize)]
pub struct ModeUpdate {
    pub mode: String,
}

/// Autonomy update request.
#[derive(Debug, Deserialize)]
pub struct AutonomyUpdate {
    pub level: String,
}

/// Verify model request.
#[derive(Debug, Deserialize)]
pub struct VerifyModelRequest {
    pub provider: String,
    pub model: String,
}

/// Build the config router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/config", get(get_config).put(update_config))
        .route("/api/config/mode", post(set_mode))
        .route("/api/config/autonomy", post(set_autonomy))
        .route("/api/config/providers", get(list_providers))
        .route("/api/config/verify-model", post(verify_model))
}

/// Get current configuration.
async fn get_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let config = state.config().await;

    // Mask API key
    let masked_key = config.api_key.as_ref().map(|key| {
        if key.len() > 8 {
            format!("{}...{}", &key[..4], &key[key.len() - 4..])
        } else {
            "***".to_string()
        }
    });

    let mode = state.mode().await;
    let autonomy_level = state.autonomy_level().await;
    let git_branch = state.git_branch();

    // Resolve compact agent role for API consumers
    let (compact_model, compact_provider) = config.resolve_agent_role("compact");
    let compact_model_opt =
        if compact_model == config.model && compact_provider == config.model_provider {
            None
        } else {
            Some(&compact_model)
        };
    let compact_provider_opt = if compact_model_opt.is_none() {
        None
    } else {
        Some(&compact_provider)
    };

    Ok(Json(serde_json::json!({
        "model_provider": config.model_provider,
        "model": config.model,
        "model_vlm_provider": config.model_vlm_provider,
        "model_vlm": config.model_vlm,
        "model_compact_provider": compact_provider_opt,
        "model_compact": compact_model_opt,
        "api_key": masked_key,
        "temperature": config.temperature,
        "max_tokens": config.max_tokens,
        "enable_bash": config.enable_bash,
        "mode": mode.to_string(),
        "autonomy_level": autonomy_level,
        "working_dir": state.working_dir(),
        "git_branch": git_branch,
    })))
}

/// Update configuration.
async fn update_config(
    State(state): State<AppState>,
    Json(update): Json<ConfigUpdate>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mut config = state.config_mut().await;

    if let Some(provider) = update.model_provider {
        config.model_provider = provider;
    }
    if let Some(model) = update.model {
        config.model = model;
    }
    if let Some(provider) = update.model_vlm_provider {
        config.model_vlm_provider = Some(provider);
    }
    if let Some(model) = update.model_vlm {
        config.model_vlm = Some(model);
    }
    if let Some(temp) = update.temperature {
        config.temperature = temp;
    }
    if let Some(max) = update.max_tokens {
        config.max_tokens = max;
    }
    if let Some(bash) = update.enable_bash {
        config.enable_bash = bash;
    }

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Configuration updated",
    })))
}

/// Set operation mode.
async fn set_mode(
    State(state): State<AppState>,
    Json(update): Json<ModeUpdate>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mode = match update.mode.as_str() {
        "normal" => OperationMode::Normal,
        "plan" => OperationMode::Plan,
        other => {
            return Err(WebError::BadRequest(format!("Invalid mode: {}", other)));
        }
    };

    state.set_mode(mode).await;

    state.broadcast(WsBroadcast::new(
        WsMessageType::StatusUpdate.as_str().to_string(),
        serde_json::json!({
        "mode": mode.to_string(),
        "autonomy_level": state.autonomy_level().await,
        }),
    ));

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": format!("Mode set to {}", mode),
    })))
}

/// Set autonomy level.
async fn set_autonomy(
    State(state): State<AppState>,
    Json(update): Json<AutonomyUpdate>,
) -> Result<Json<serde_json::Value>, WebError> {
    let valid = ["Manual", "Semi-Auto", "Auto"];
    if !valid.contains(&update.level.as_str()) {
        return Err(WebError::BadRequest(format!(
            "Invalid autonomy level: {}. Must be one of {:?}",
            update.level, valid
        )));
    }

    state.set_autonomy_level(update.level.clone()).await;

    state.broadcast(WsBroadcast::new(
        WsMessageType::StatusUpdate.as_str().to_string(),
        serde_json::json!({
        "mode": state.mode().await.to_string(),
        "autonomy_level": update.level,
        }),
    ));

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": format!("Autonomy set to {}", update.level),
    })))
}

/// List all available AI providers and their models.
async fn list_providers(
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, WebError> {
    let registry = state.model_registry().await;

    let mut providers = Vec::new();
    for provider_info in registry.list_providers() {
        let models: Vec<serde_json::Value> = provider_info
            .list_models(None)
            .iter()
            .map(|model_info| {
                let ctx_k = model_info.context_length / 1000;
                let mut description = format!("{}k context", ctx_k);
                if model_info.recommended {
                    description = format!("Recommended \u{2022} {}", description);
                }

                serde_json::json!({
                    "id": model_info.id,
                    "name": model_info.name,
                    "description": description,
                })
            })
            .collect();

        providers.push(serde_json::json!({
            "id": provider_info.id,
            "name": provider_info.name,
            "description": provider_info.description,
            "models": models,
        }));
    }

    Ok(Json(providers))
}

/// Verify that a provider/model combination is accessible.
async fn verify_model(
    State(state): State<AppState>,
    Json(request): Json<VerifyModelRequest>,
) -> Json<serde_json::Value> {
    let registry = state.model_registry().await;

    let provider = match registry.get_provider(&request.provider) {
        Some(p) => p,
        None => {
            return Json(serde_json::json!({
                "valid": false,
                "error": format!("Unknown provider: {}", request.provider),
            }));
        }
    };

    if request.model.is_empty() {
        return Json(serde_json::json!({
            "valid": false,
            "error": "Model name cannot be empty",
        }));
    }

    let _model_found = registry.find_model_by_id(&request.model).is_some();

    let config = state.config().await;
    let env_var = &provider.api_key_env;
    let has_key = if env_var.is_empty() {
        config.api_key.is_some()
    } else {
        config.api_key.is_some() || std::env::var(env_var).is_ok()
    };

    if !has_key {
        let hint = if env_var.is_empty() {
            "No API key configured".to_string()
        } else {
            format!("No API key found. Set {} environment variable", env_var)
        };
        return Json(serde_json::json!({
            "valid": false,
            "error": hint,
        }));
    }

    Json(serde_json::json!({
        "valid": true,
    }))
}
