use axum::body::Body;
use axum::http::{Request, StatusCode};
use opendev_config::ModelRegistry;
use opendev_history::SessionManager;
use opendev_http::UserStore;
use opendev_models::AppConfig;
use tempfile::TempDir;
use tower::ServiceExt;

use crate::state::AppState;

fn make_state() -> AppState {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.into_path();
    let session_manager = SessionManager::new(tmp_path.clone()).unwrap();
    let config = AppConfig::default();
    let user_store = UserStore::new(tmp_path).unwrap();
    let model_registry = ModelRegistry::new();
    AppState::new(
        session_manager,
        config,
        "/tmp/test".to_string(),
        user_store,
        model_registry,
    )
}

#[tokio::test]
async fn test_list_commands() {
    let state = make_state();
    let app = crate::server::build_app(state, None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/commands")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_array());
    assert!(!json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_help() {
    let state = make_state();
    let app = crate::server::build_app(state, None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/commands/help")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["title"], "Available Commands");
    assert!(json["commands"].is_array());
}
