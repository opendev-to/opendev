use super::*;

#[test]
fn test_local_runtime_new() {
    let rt = LocalRuntime::new("abc123");
    assert_eq!(rt.container_id(), "abc123");
    assert!(!rt.closed);
}

#[test]
fn test_is_alive() {
    let rt = LocalRuntime::new("abc123");
    let resp = rt.is_alive();
    assert_eq!(resp.status, "ok");
}

#[test]
fn test_is_alive_when_closed() {
    let mut rt = LocalRuntime::new("abc123");
    rt.closed = true;
    let resp = rt.is_alive();
    assert_eq!(resp.status, "error");
}

#[tokio::test]
async fn test_close_session_not_found() {
    let mut rt = LocalRuntime::new("abc123");
    let resp = rt
        .close_session(&CloseSessionRequest {
            session: "nonexistent".into(),
        })
        .await;
    assert!(!resp.success);
}

#[tokio::test]
async fn test_create_session_duplicate() {
    let mut rt = LocalRuntime::new("abc123");
    // Manually insert a session
    rt.sessions
        .insert("default".into(), DockerSession::new("abc123", "default"));
    let err = rt
        .create_session(&CreateSessionRequest::default())
        .await
        .unwrap_err();
    assert!(matches!(err, DockerError::SessionExists(_)));
}

#[tokio::test]
async fn test_close_runtime() {
    let mut rt = LocalRuntime::new("abc123");
    rt.close().await;
    assert!(rt.closed);
    let resp = rt.is_alive();
    assert_eq!(resp.status, "error");
}
