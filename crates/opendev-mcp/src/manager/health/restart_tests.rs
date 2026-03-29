use super::*;
use crate::config::McpConfig;
use crate::manager::{MAX_RESTART_ATTEMPTS, ServerHealthState};
use std::path::PathBuf;

#[tokio::test]
async fn test_permanently_failed_not_restarted() {
    let manager = McpManager::new(None);

    {
        let mut states = manager.health_states.write().await;
        states.insert(
            "dead-server".to_string(),
            ServerHealthState {
                status: ServerHealthStatus::PermanentlyFailed,
                consecutive_failures: 10,
                restart_attempts: MAX_RESTART_ATTEMPTS,
                tools_removed: true,
            },
        );
    }

    let restarted = manager.attempt_restart("dead-server").await;
    assert!(!restarted);
}

#[tokio::test]
async fn test_restart_increments_attempts() {
    let manager = McpManager::new(Some(PathBuf::from("/tmp")));

    {
        let mut config = manager.config.write().await;
        *config = Some(McpConfig::default());
    }

    {
        let mut states = manager.health_states.write().await;
        states.insert(
            "restart-test".to_string(),
            ServerHealthState {
                status: ServerHealthStatus::Unhealthy,
                consecutive_failures: 3,
                restart_attempts: 0,
                tools_removed: true,
            },
        );
    }

    let result = manager.attempt_restart("restart-test").await;
    assert!(!result);

    let state = manager.get_health_state("restart-test").await.unwrap();
    assert_eq!(state.restart_attempts, 1);
}

#[tokio::test]
async fn test_max_restart_attempts_marks_permanently_failed() {
    let manager = McpManager::new(Some(PathBuf::from("/tmp")));

    {
        let mut config = manager.config.write().await;
        *config = Some(McpConfig::default());
    }

    {
        let mut states = manager.health_states.write().await;
        states.insert(
            "doomed".to_string(),
            ServerHealthState {
                status: ServerHealthStatus::Unhealthy,
                consecutive_failures: 5,
                restart_attempts: MAX_RESTART_ATTEMPTS - 1,
                tools_removed: true,
            },
        );
    }

    let result = manager.attempt_restart("doomed").await;
    assert!(!result);

    let state = manager.get_health_state("doomed").await.unwrap();
    assert_eq!(state.status, ServerHealthStatus::PermanentlyFailed);
}
