use super::*;
use crate::manager::HEALTH_CHECK_FAILURE_THRESHOLD;

#[tokio::test]
async fn test_health_state_default() {
    let state = ServerHealthState::default();
    assert_eq!(state.status, ServerHealthStatus::Healthy);
    assert_eq!(state.consecutive_failures, 0);
    assert_eq!(state.restart_attempts, 0);
    assert!(!state.tools_removed);
}

#[test]
fn test_backoff_calculation() {
    let mut state = ServerHealthState::default();
    assert_eq!(state.next_backoff_secs(), 1); // 2^0 = 1

    state.restart_attempts = 1;
    assert_eq!(state.next_backoff_secs(), 2); // 2^1 = 2

    state.restart_attempts = 2;
    assert_eq!(state.next_backoff_secs(), 4); // 2^2 = 4

    state.restart_attempts = 3;
    assert_eq!(state.next_backoff_secs(), 8); // 2^3 = 8

    // Should cap at MAX_BACKOFF_SECS.
    state.restart_attempts = 10;
    assert_eq!(state.next_backoff_secs(), MAX_BACKOFF_SECS);
}

#[tokio::test]
async fn test_health_check_success_resets_failures() {
    let manager = McpManager::new(None);

    {
        let mut states = manager.health_states.write().await;
        states.insert(
            "test-server".to_string(),
            ServerHealthState {
                status: ServerHealthStatus::Degraded,
                consecutive_failures: 2,
                restart_attempts: 0,
                tools_removed: false,
            },
        );
    }

    manager.record_health_check("test-server", true).await;

    let state = manager.get_health_state("test-server").await.unwrap();
    assert_eq!(state.status, ServerHealthStatus::Healthy);
    assert_eq!(state.consecutive_failures, 0);
}

#[tokio::test]
async fn test_health_check_failures_degrade_then_unhealthy() {
    let manager = McpManager::new(None);

    {
        let mut states = manager.health_states.write().await;
        states.insert("test-server".to_string(), ServerHealthState::default());
    }

    for _ in 0..HEALTH_CHECK_FAILURE_THRESHOLD - 1 {
        manager.record_health_check("test-server", false).await;
    }

    let state = manager.get_health_state("test-server").await.unwrap();
    assert_eq!(state.status, ServerHealthStatus::Degraded);
    assert_eq!(
        state.consecutive_failures,
        HEALTH_CHECK_FAILURE_THRESHOLD - 1
    );

    manager.record_health_check("test-server", false).await;

    let state = manager.get_health_state("test-server").await.unwrap();
    assert_eq!(state.status, ServerHealthStatus::Unhealthy);
    assert!(state.tools_removed);
}

#[tokio::test]
async fn test_ping_nonexistent_server_returns_false() {
    let manager = McpManager::new(None);
    assert!(!manager.ping_server("nonexistent").await);
}

#[tokio::test]
async fn test_get_all_health_states() {
    let manager = McpManager::new(None);

    {
        let mut states = manager.health_states.write().await;
        states.insert("s1".to_string(), ServerHealthState::default());
        states.insert(
            "s2".to_string(),
            ServerHealthState {
                status: ServerHealthStatus::Unhealthy,
                consecutive_failures: 3,
                restart_attempts: 1,
                tools_removed: true,
            },
        );
    }

    let all = manager.get_all_health_states().await;
    assert_eq!(all.len(), 2);
    assert_eq!(all["s1"].status, ServerHealthStatus::Healthy);
    assert_eq!(all["s2"].status, ServerHealthStatus::Unhealthy);
}

#[tokio::test]
async fn test_get_health_state_unknown_server() {
    let manager = McpManager::new(None);
    let state = manager.get_health_state("unknown").await;
    assert!(state.is_none());
}

#[tokio::test]
async fn test_health_monitoring_disabled_when_zero() {
    let manager = McpManager::new(None).with_health_check_interval(0);
    manager.start_health_monitoring().await;
    let handle = manager.health_check_handle.read().await;
    assert!(handle.is_none());
}

#[tokio::test]
async fn test_stop_health_monitoring_when_not_running() {
    let manager = McpManager::new(None);
    manager.stop_health_monitoring().await;
    let handle = manager.health_check_handle.read().await;
    assert!(handle.is_none());
}

#[tokio::test]
async fn test_stop_health_monitoring_clears_handle() {
    let manager = McpManager::new(None);
    {
        let mut handle = manager.health_check_handle.write().await;
        *handle = Some(tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }));
    }
    assert!(manager.health_check_handle.read().await.is_some());

    manager.stop_health_monitoring().await;
    assert!(manager.health_check_handle.read().await.is_none());
}
