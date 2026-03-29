use super::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use opendev_history::SessionManager;
use opendev_models::AppConfig;
use tempfile::TempDir;
use tower::ServiceExt;

fn make_state_with_workdir(work_dir: &str) -> AppState {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.into_path();
    let session_manager = SessionManager::new(tmp_path.clone()).unwrap();
    let config = AppConfig::default();
    let user_store = opendev_http::UserStore::new(tmp_path).unwrap();
    let model_registry = opendev_config::ModelRegistry::new();
    AppState::new(
        session_manager,
        config,
        work_dir.to_string(),
        user_store,
        model_registry,
    )
}

#[tokio::test]
async fn test_list_servers_empty() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_workdir(&tmp.path().to_string_lossy());

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/mcp/servers")
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
    // May or may not have servers depending on user's ~/.opendev/mcp.json
    assert!(json["servers"].is_array());
}

#[tokio::test]
async fn test_get_server_not_found() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_workdir(&tmp.path().to_string_lossy());

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/mcp/servers/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_and_get_server() {
    let tmp = TempDir::new().unwrap();
    // Use a temp dir as working dir so we don't write to real ~/.opendev/
    let work_dir = tmp.path().to_string_lossy().to_string();

    // Override HOME to the temp dir so global_config_path resolves there.
    // SAFETY: test-only; overrides HOME so config resolves to temp dir.
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let state = make_state_with_workdir(&work_dir);

    // Create the .opendev directory.
    std::fs::create_dir_all(tmp.path().join(".opendev")).unwrap();

    // Create server.
    let app = crate::server::build_app(state.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/mcp/servers")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"name":"test-server","command":"uvx","args":["mcp-server-test"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Get server.
    let app = crate::server::build_app(state.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/mcp/servers/test-server")
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
    assert_eq!(json["name"], "test-server");
    assert_eq!(json["config"]["command"], "uvx");

    // Delete server.
    let app = crate::server::build_app(state.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/mcp/servers/test-server")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_connect_server_not_found() {
    let tmp = TempDir::new().unwrap();
    let state = make_state_with_workdir(&tmp.path().to_string_lossy());

    let app = crate::server::build_app(state, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/mcp/servers/nonexistent/connect")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
