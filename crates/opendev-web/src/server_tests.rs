use super::*;
use crate::state::AppState;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use opendev_config::ModelRegistry;
use opendev_history::SessionManager;
use opendev_http::UserStore;
use opendev_models::AppConfig;
use tempfile::TempDir;
use tower::ServiceExt;

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
async fn test_health_check() {
    let state = make_state();
    let app = build_app(state, None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
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
    assert_eq!(json["status"], "ok");
}
