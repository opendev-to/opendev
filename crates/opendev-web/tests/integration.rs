//! Integration tests for the web server.
//!
//! Tests health check, session CRUD routes, and config routes
//! using Axum's test utilities (oneshot requests).

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;

use opendev_history::SessionManager;
use opendev_models::{AppConfig, Session};
use opendev_web::server::build_app;
use opendev_web::state::AppState;

use tempfile::TempDir;

/// Create a test AppState with a temp session directory.
fn make_test_state() -> (TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let session_manager = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let config = AppConfig::default();
    let user_store = opendev_http::UserStore::new(tmp.path().to_path_buf()).unwrap();
    let model_registry = opendev_config::ModelRegistry::new();
    let state = AppState::new(
        session_manager,
        config,
        tmp.path().to_string_lossy().to_string(),
        user_store,
        model_registry,
    );
    (tmp, state)
}

/// Helper: send a request and get the response.
async fn send_request(
    state: AppState,
    method: Method,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let app = build_app(state, None);

    let mut builder = Request::builder().method(method).uri(uri);

    let body = if let Some(json) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(serde_json::to_string(&json).unwrap())
    } else {
        Body::empty()
    };

    let request = builder.body(body).unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::json!(null));

    (status, json)
}

// ========================================================================
// Health check
// ========================================================================

/// Health check returns 200 with status ok.
#[tokio::test]
async fn health_check_returns_ok() {
    let (_tmp, state) = make_test_state();
    let (status, json) = send_request(state, Method::GET, "/api/health", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "opendev-web-ui");
}

// ========================================================================
// Session CRUD routes
// ========================================================================

/// Create a session via POST /api/sessions.
#[tokio::test]
async fn create_session() {
    let (_tmp, state) = make_test_state();
    let (status, json) = send_request(
        state,
        Method::POST,
        "/api/sessions",
        Some(serde_json::json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "created");
    assert!(json["id"].as_str().is_some(), "should return session ID");
}

/// Create session with working_directory.
#[tokio::test]
async fn create_session_with_working_dir() {
    let (_tmp, state) = make_test_state();
    let (status, json) = send_request(
        state,
        Method::POST,
        "/api/sessions",
        Some(serde_json::json!({"working_directory": "/tmp/my-project"})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["id"].as_str().is_some());
}

/// List sessions returns empty array initially.
#[tokio::test]
async fn list_sessions_empty() {
    let (_tmp, state) = make_test_state();
    let (status, json) = send_request(state, Method::GET, "/api/sessions", None).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.as_array().is_some());
    assert!(json.as_array().unwrap().is_empty());
}

/// List sessions returns created sessions.
#[tokio::test]
async fn list_sessions_after_create() {
    let (tmp, state) = make_test_state();

    // Manually save a session so it appears in the index
    {
        let mut mgr = state.session_manager_mut().await;
        let session = mgr.create_session();
        let _ = session; // create_session sets it as current
        mgr.save_current().unwrap();
    }

    let (status, json) = send_request(state, Method::GET, "/api/sessions", None).await;
    assert_eq!(status, StatusCode::OK);
    let sessions = json.as_array().unwrap();
    assert_eq!(sessions.len(), 1);
}

/// Get a specific session by ID.
#[tokio::test]
async fn get_session_by_id() {
    let (tmp, state) = make_test_state();

    // Save a session
    let session_id;
    {
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
        let mut session = Session::new();
        session.id = "test-get".to_string();
        mgr.save_session(&session).unwrap();
        session_id = session.id;
    }

    let (status, json) = send_request(
        state,
        Method::GET,
        &format!("/api/sessions/{session_id}"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], session_id);
}

/// Get nonexistent session returns 404.
#[tokio::test]
async fn get_nonexistent_session_returns_404() {
    let (_tmp, state) = make_test_state();
    let (status, json) = send_request(state, Method::GET, "/api/sessions/nonexistent", None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(json["error"].as_str().is_some());
}

/// Resume a session.
#[tokio::test]
async fn resume_session() {
    let (tmp, state) = make_test_state();

    // Save a session
    {
        let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
        let mut session = Session::new();
        session.id = "test-resume".to_string();
        mgr.save_session(&session).unwrap();
    }

    let (status, json) = send_request(
        state,
        Method::POST,
        "/api/sessions/test-resume/resume",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "resumed");
    assert_eq!(json["session_id"], "test-resume");
}

/// Resume nonexistent session returns 404.
#[tokio::test]
async fn resume_nonexistent_session_returns_404() {
    let (_tmp, state) = make_test_state();
    let (status, _) = send_request(
        state,
        Method::POST,
        "/api/sessions/nonexistent/resume",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ========================================================================
// Config routes
// ========================================================================

/// GET /api/config returns current configuration.
#[tokio::test]
async fn get_config() {
    let (_tmp, state) = make_test_state();
    let (status, json) = send_request(state, Method::GET, "/api/config", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["model_provider"], "fireworks");
    assert_eq!(json["mode"], "normal");
    assert_eq!(json["autonomy_level"], "Manual");
}

/// PUT /api/config updates configuration.
#[tokio::test]
async fn update_config() {
    let (_tmp, state) = make_test_state();

    let (status, json) = send_request(
        state.clone(),
        Method::PUT,
        "/api/config",
        Some(serde_json::json!({
            "model_provider": "openai",
            "model": "gpt-4o",
            "temperature": 0.9
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "success");

    // Verify the config was updated
    let (status, json) = send_request(state, Method::GET, "/api/config", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["model_provider"], "openai");
    assert_eq!(json["model"], "gpt-4o");
}

/// POST /api/config/mode sets operation mode.
#[tokio::test]
async fn set_mode() {
    let (_tmp, state) = make_test_state();

    let (status, json) = send_request(
        state.clone(),
        Method::POST,
        "/api/config/mode",
        Some(serde_json::json!({"mode": "plan"})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "success");

    // Verify mode changed
    let (_, config) = send_request(state, Method::GET, "/api/config", None).await;
    assert_eq!(config["mode"], "plan");
}

/// POST /api/config/mode with invalid mode returns 400.
#[tokio::test]
async fn set_invalid_mode_returns_400() {
    let (_tmp, state) = make_test_state();

    let (status, json) = send_request(
        state,
        Method::POST,
        "/api/config/mode",
        Some(serde_json::json!({"mode": "invalid"})),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().is_some());
}

/// POST /api/config/autonomy sets autonomy level.
#[tokio::test]
async fn set_autonomy_level() {
    let (_tmp, state) = make_test_state();

    let (status, json) = send_request(
        state.clone(),
        Method::POST,
        "/api/config/autonomy",
        Some(serde_json::json!({"level": "Auto"})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    let (_, config) = send_request(state, Method::GET, "/api/config", None).await;
    assert_eq!(config["autonomy_level"], "Auto");
}

/// POST /api/config/autonomy with invalid level returns 400.
#[tokio::test]
async fn set_invalid_autonomy_returns_400() {
    let (_tmp, state) = make_test_state();

    let (status, _) = send_request(
        state,
        Method::POST,
        "/api/config/autonomy",
        Some(serde_json::json!({"level": "SuperAuto"})),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ========================================================================
// AppState tests
// ========================================================================

/// Interrupt flag lifecycle.
#[tokio::test]
async fn state_interrupt_flag() {
    let (_tmp, state) = make_test_state();

    assert!(!state.is_interrupt_requested().await);
    state.request_interrupt().await;
    assert!(state.is_interrupt_requested().await);
    state.clear_interrupt().await;
    assert!(!state.is_interrupt_requested().await);
}

/// Session running state tracking.
#[tokio::test]
async fn state_session_running() {
    let (_tmp, state) = make_test_state();

    assert!(!state.is_session_running("s1").await);
    state.set_session_running("s1".to_string()).await;
    assert!(state.is_session_running("s1").await);
    state.set_session_idle("s1").await;
    assert!(!state.is_session_running("s1").await);
}

/// WebSocket broadcast delivers messages.
#[tokio::test]
async fn state_ws_broadcast() {
    let (_tmp, state) = make_test_state();
    let mut rx = state.ws_subscribe();

    state.broadcast(opendev_web::state::WsBroadcast::new(
        "test_event",
        serde_json::json!({"key": "value"}),
    ));

    let msg = rx.recv().await.unwrap();
    assert_eq!(msg.msg_type, "test_event");
    assert_eq!(msg.data["key"], "value");
}
