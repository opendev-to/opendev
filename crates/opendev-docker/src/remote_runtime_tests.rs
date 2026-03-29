use super::*;

#[test]
fn test_remote_runtime_new() {
    let rt = RemoteRuntime::new("example.com", Some("deploy"), None);
    assert_eq!(rt.ssh_target, "deploy@example.com");
    assert!(rt.container_id.is_none());
    assert!(!rt.closed);
}

#[test]
fn test_remote_runtime_no_user() {
    let rt = RemoteRuntime::new("10.0.0.1", None, None);
    assert_eq!(rt.ssh_target, "10.0.0.1");
}

#[test]
fn test_set_container_id() {
    let mut rt = RemoteRuntime::new("host", None, None);
    rt.set_container_id("abc123");
    assert_eq!(rt.container_id.as_deref(), Some("abc123"));
}

#[test]
fn test_from_config_missing_host() {
    let cfg = DockerConfig::default();
    let err = RemoteRuntime::from_config(&cfg).unwrap_err();
    assert!(err.to_string().contains("remote_host"));
}

#[test]
fn test_from_config_with_host() {
    let mut cfg = DockerConfig::default();
    cfg.remote_host = Some("myhost.com".into());
    cfg.remote_user = Some("user".into());
    cfg.ssh_key_path = Some("/home/user/.ssh/id_rsa".into());
    let rt = RemoteRuntime::from_config(&cfg).unwrap();
    assert_eq!(rt.ssh_target, "user@myhost.com");
    assert_eq!(rt.ssh_key_path.as_deref(), Some("/home/user/.ssh/id_rsa"));
}

#[tokio::test]
async fn test_close_remote_runtime() {
    let mut rt = RemoteRuntime::new("host", None, None);
    rt.close().await;
    assert!(rt.closed);
    let resp = rt.is_alive().await;
    assert_eq!(resp.status, "error");
}

#[tokio::test]
async fn test_exec_no_container_id() {
    let rt = RemoteRuntime::new("host", None, None);
    let err = rt.exec_in_container("ls", 10.0).await.unwrap_err();
    assert!(err.to_string().contains("No container ID"));
}
