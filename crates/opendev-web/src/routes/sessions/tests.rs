use super::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use opendev_config::ModelRegistry;
use opendev_history::SessionManager;
use opendev_http::UserStore;
use opendev_models::AppConfig;
use tempfile::TempDir;
use tower::ServiceExt;

fn make_state_with_dir(tmp: &std::path::Path) -> AppState {
    let session_manager = SessionManager::new(tmp.to_path_buf()).unwrap();
    let config = AppConfig::default();
    let user_store = UserStore::new(tmp.to_path_buf()).unwrap();
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
async fn test_session_reuse_empty_session() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_dir(tmp.path());

    // Create an empty session with a workspace.
    {
        let mut mgr = state.session_manager_mut().await;
        let _session = mgr.create_session();
        mgr.current_session_mut().unwrap().working_directory =
            Some("/workspace/project".to_string());
        mgr.save_current().unwrap();
        // Clear current so it doesn't interfere with the stale check.
        drop(mgr);
    }

    // Get the first session ID from the index.
    let first_session_id = {
        let mgr = state.session_manager().await;
        let index = mgr.index().read_index().unwrap();
        assert_eq!(index.entries.len(), 1);
        index.entries[0].session_id.clone()
    };

    // Now POST /api/sessions with the same workspace.
    let app = crate::server::build_app(state.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sessions")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"working_directory":"/workspace/project"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "reused");
    assert_eq!(json["id"], first_session_id);
}

#[tokio::test]
async fn test_session_create_new_when_no_empty_match() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_dir(tmp.path());

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sessions")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"working_directory":"/workspace/new-project"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "created");
}

#[tokio::test]
async fn test_delete_session() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_dir(tmp.path());

    // Create a session.
    let session_id = {
        let mut mgr = state.session_manager_mut().await;
        let session = mgr.create_session();
        let id = session.id.clone();
        mgr.save_current().unwrap();
        id
    };

    let app = crate::server::build_app(state.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/sessions/{}", session_id))
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
    assert_eq!(json["status"], "success");
}

#[tokio::test]
async fn test_delete_session_not_found() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_dir(tmp.path());

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/sessions/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_bridge_info() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_dir(tmp.path());

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/sessions/bridge-info")
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
    assert_eq!(json["bridge_mode"], false);
}

#[tokio::test]
async fn test_verify_path_empty() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_dir(tmp.path());

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sessions/verify-path")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"path":""}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["exists"], false);
}

#[cfg(unix)]
#[tokio::test]
async fn test_verify_path_valid_dir() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_dir(tmp.path());

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sessions/verify-path")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"path":"{}"}}"#,
                    tmp.path().to_string_lossy()
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
    assert_eq!(json["exists"], true);
    assert_eq!(json["is_directory"], true);
}

#[cfg(unix)]
#[tokio::test]
async fn test_browse_directory() {
    let tmp = TempDir::new().unwrap();
    // Create a subdirectory.
    std::fs::create_dir(tmp.path().join("subdir")).unwrap();

    let state = make_state_with_dir(tmp.path());

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sessions/browse-directory")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"path":"{}","show_hidden":false}}"#,
                    tmp.path().to_string_lossy()
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
    assert!(json["error"].is_null());
    let dirs = json["directories"].as_array().unwrap();
    assert!(dirs.iter().any(|d| d["name"] == "subdir"));
}

#[tokio::test]
async fn test_session_model_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_dir(tmp.path());

    // Create a session.
    let session_id = {
        let mut mgr = state.session_manager_mut().await;
        let session = mgr.create_session();
        let id = session.id.clone();
        mgr.save_current().unwrap();
        id
    };

    // GET model — should be empty.
    let app = crate::server::build_app(state.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/sessions/{}/model", session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // PUT model.
    let app = crate::server::build_app(state.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/sessions/{}/model", session_id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"model_provider":"openai","model":"gpt-4"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // GET model — should have values.
    let app = crate::server::build_app(state.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/sessions/{}/model", session_id))
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
    assert_eq!(json["model_provider"], "openai");

    // DELETE model.
    let app = crate::server::build_app(state.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/sessions/{}/model", session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_files_no_session() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_dir(tmp.path());

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/sessions/files")
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
    assert_eq!(json["files"].as_array().unwrap().len(), 0);
}
