use super::*;

#[test]
fn test_find_free_port() {
    let port = find_free_port().unwrap();
    assert!(port > 0);
}

#[test]
fn test_deployment_new() {
    let cfg = DockerConfig::default();
    let deploy = DockerDeployment::new(cfg).unwrap();
    assert!(!deploy.is_started());
    assert!(deploy.container_id().is_none());
    assert!(deploy.container_name().starts_with("opendev-runtime-"));
    assert!(!deploy.auth_token().is_empty());
}

#[test]
fn test_deployment_with_status_callback() {
    use std::sync::{Arc, Mutex};
    let messages = Arc::new(Mutex::new(Vec::<String>::new()));
    let msgs = messages.clone();
    let cfg = DockerConfig::default();
    let deploy = DockerDeployment::new(cfg)
        .unwrap()
        .with_status_callback(move |s: &str| {
            msgs.lock().unwrap().push(s.to_string());
        });
    (deploy.on_status)("test message");
    assert_eq!(messages.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_stop_when_not_started() {
    let cfg = DockerConfig::default();
    let mut deploy = DockerDeployment::new(cfg).unwrap();
    // Should be a no-op, not an error
    deploy.stop().await.unwrap();
}

#[tokio::test]
async fn test_inspect_when_not_started() {
    let cfg = DockerConfig::default();
    let deploy = DockerDeployment::new(cfg).unwrap();
    let status = deploy.inspect().await.unwrap();
    assert_eq!(status, ContainerStatus::Unknown);
}

#[tokio::test]
async fn test_remove_when_not_started() {
    let cfg = DockerConfig::default();
    let mut deploy = DockerDeployment::new(cfg).unwrap();
    deploy.remove().await.unwrap();
}
