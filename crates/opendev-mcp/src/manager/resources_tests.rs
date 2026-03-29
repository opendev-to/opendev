use super::*;

#[tokio::test]
async fn test_list_prompts_no_connections() {
    let manager = McpManager::new(None);
    let prompts = manager.list_prompts().await;
    assert!(prompts.is_empty());
}

#[tokio::test]
async fn test_get_prompt_disconnected_server() {
    let manager = McpManager::new(None);
    let result = manager.get_prompt("nonexistent", "test-prompt", None).await;
    assert!(matches!(result, Err(McpError::ServerNotFound(_))));
}

#[tokio::test]
async fn test_get_prompt_with_arguments_disconnected() {
    let manager = McpManager::new(None);
    let mut args = HashMap::new();
    args.insert("key".to_string(), "value".to_string());
    let result = manager
        .get_prompt("nonexistent", "test-prompt", Some(args))
        .await;
    assert!(matches!(result, Err(McpError::ServerNotFound(_))));
}

#[tokio::test]
async fn test_list_resources_no_connections() {
    let manager = McpManager::new(None);
    let resources = manager.list_resources().await;
    assert!(resources.is_empty());
}

#[tokio::test]
async fn test_read_resource_disconnected_server() {
    let manager = McpManager::new(None);
    let result = manager
        .read_resource("nonexistent", "file:///test.txt")
        .await;
    assert!(matches!(result, Err(McpError::ServerNotFound(_))));
}
