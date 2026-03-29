use super::*;
use std::path::PathBuf;

#[tokio::test]
async fn test_manager_new() {
    let manager = McpManager::new(None);
    assert_eq!(manager.connected_count().await, 0);
}

#[tokio::test]
async fn test_with_health_check_interval() {
    let manager = McpManager::new(None).with_health_check_interval(60);
    assert_eq!(manager.health_check_interval_secs, 60);
}

#[tokio::test]
async fn test_acquire_oauth_token_invalid_url() {
    let oauth = McpOAuthConfig {
        client_id: "test-client".to_string(),
        client_secret: "test-secret".to_string(),
        token_url: "http://127.0.0.1:1/nonexistent/token".to_string(),
        scope: Some("read write".to_string()),
    };
    let result = McpManager::acquire_oauth_token(&oauth).await;
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(err_str.contains("OAuth token request failed"));
}

#[test]
fn test_oauth_config_struct() {
    let oauth = McpOAuthConfig {
        client_id: "cid".to_string(),
        client_secret: "csecret".to_string(),
        token_url: "https://auth.example.com/token".to_string(),
        scope: Some("mcp:tools".to_string()),
    };
    assert_eq!(oauth.client_id, "cid");
    assert_eq!(oauth.scope.as_deref(), Some("mcp:tools"));

    let oauth2 = oauth.clone();
    assert_eq!(oauth, oauth2);
}

#[test]
fn test_oauth_config_serialization() {
    let oauth = McpOAuthConfig {
        client_id: "cid".to_string(),
        client_secret: "csecret".to_string(),
        token_url: "https://auth.example.com/token".to_string(),
        scope: None,
    };
    let json = serde_json::to_string(&oauth).unwrap();
    let parsed: McpOAuthConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.client_id, "cid");
    assert!(parsed.scope.is_none());
}

#[tokio::test]
async fn test_connect_nonexistent_server() {
    let manager = McpManager::new(Some(PathBuf::from("/tmp")));
    {
        let mut config = manager.config.write().await;
        *config = Some(McpConfig::default());
    }

    let result = manager.connect_server("nonexistent").await;
    assert!(matches!(result, Err(McpError::ServerNotFound(_))));
}
