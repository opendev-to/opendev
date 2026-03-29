use super::*;
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
async fn test_send_query_empty_message_returns_400() {
    let state = make_state();
    let app = crate::server::build_app(state, None);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/query")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"message":"  ","session_id":"s1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_send_query_no_session_returns_400() {
    let state = make_state();
    let app = crate::server::build_app(state, None);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/query")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"message":"hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // No session -> 400.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_send_query_session_not_found_returns_404() {
    let state = make_state();
    let app = crate::server::build_app(state, None);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/query")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"message":"hello","session_id":"nonexistent"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_send_query_running_session_inject() {
    let state = make_state();

    // Create a session.
    {
        let mut mgr = state.session_manager_mut().await;
        let _session = mgr.create_session();
        mgr.save_current().unwrap();
    }

    let session_id = state.current_session_id().await.unwrap();

    // Mark session running and create injection queue.
    state.set_session_running(session_id.clone()).await;
    let (_tx, rx) = state.get_or_create_injection_queue(&session_id).await;
    let mut rx = rx.unwrap();

    let app = crate::server::build_app(state, None);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/query")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"message":"injected msg","session_id":"{}"}}"#,
                    session_id
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "accepted");

    // Verify message was injected.
    let injected = rx.recv().await.unwrap();
    assert_eq!(injected, "injected msg");
}

#[tokio::test]
async fn test_interrupt_denies_approvals() {
    let state = make_state();
    let approval = crate::state::PendingApproval {
        tool_name: "bash".to_string(),
        arguments: serde_json::json!({}),
        session_id: None,
    };

    let rx = state.add_pending_approval("a1".to_string(), approval).await;

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/interrupt")
                .header("content-type", "application/json")
                .body(Body::from(r#"{}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Approval should have been denied.
    let result = rx.await.unwrap();
    assert!(!result.approved);
}

#[tokio::test]
async fn test_clear_chat() {
    let state = make_state();

    // Create an initial session.
    {
        let mut mgr = state.session_manager_mut().await;
        mgr.create_session();
        mgr.save_current().unwrap();
    }

    let original_id = state.current_session_id().await.unwrap();

    let app = crate::server::build_app(state.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/chat/clear")
                .header("content-type", "application/json")
                .body(Body::from(r#"{}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "success");

    // New session should be different.
    let new_id = state.current_session_id().await.unwrap();
    assert_ne!(original_id, new_id);
}
