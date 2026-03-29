use super::*;
use crate::models::McpTool;
use std::path::PathBuf;

#[tokio::test]
async fn test_get_all_tool_schemas_empty() {
    let manager = McpManager::new(None);
    let schemas = manager.get_all_tool_schemas().await;
    assert!(schemas.is_empty());
}

#[tokio::test]
async fn test_call_tool_graceful_degradation_on_missing_server() {
    let manager = McpManager::new(Some(PathBuf::from("/tmp")));
    let result = manager
        .call_tool("gone", "some_tool", serde_json::json!({}))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_tool_cache_invalidation() {
    let manager = McpManager::new(None);

    {
        let mut cache = manager.tool_schema_cache.write().await;
        cache.insert(
            "test-server".to_string(),
            ToolSchemaCache {
                tools: vec![McpTool {
                    name: "cached_tool".to_string(),
                    description: "A cached tool".to_string(),
                    input_schema: serde_json::json!({}),
                }],
                invalidated: false,
            },
        );
    }

    {
        let cache = manager.tool_schema_cache.read().await;
        let entry = cache.get("test-server").unwrap();
        assert!(!entry.invalidated);
        assert_eq!(entry.tools.len(), 1);
    }

    manager.invalidate_tool_cache("test-server").await;

    {
        let cache = manager.tool_schema_cache.read().await;
        let entry = cache.get("test-server").unwrap();
        assert!(entry.invalidated);
    }
}

#[tokio::test]
async fn test_invalidate_all_tool_caches() {
    let manager = McpManager::new(None);

    {
        let mut cache = manager.tool_schema_cache.write().await;
        cache.insert(
            "server-a".to_string(),
            ToolSchemaCache {
                tools: vec![],
                invalidated: false,
            },
        );
        cache.insert(
            "server-b".to_string(),
            ToolSchemaCache {
                tools: vec![],
                invalidated: false,
            },
        );
    }

    manager.invalidate_all_tool_caches().await;

    let cache = manager.tool_schema_cache.read().await;
    assert!(cache.get("server-a").unwrap().invalidated);
    assert!(cache.get("server-b").unwrap().invalidated);
}

#[tokio::test]
async fn test_handle_tools_changed_nonexistent_server() {
    let manager = McpManager::new(None);
    // Should not panic, just log warning
    manager.handle_tools_changed("nonexistent").await;
}

#[tokio::test]
async fn test_refresh_tools_nonexistent_server() {
    let manager = McpManager::new(None);
    let result = manager.refresh_tools("nonexistent").await;
    assert!(result.is_err());
}
