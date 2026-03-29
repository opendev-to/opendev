use super::*;
use std::path::PathBuf;

#[tokio::test]
async fn test_add_and_remove_server() {
    let manager = McpManager::new(Some(PathBuf::from("/tmp")));
    {
        let mut config = manager.config.write().await;
        *config = Some(McpConfig::default());
    }

    manager
        .add_server(
            "test-server".to_string(),
            McpServerConfig {
                command: "node".to_string(),
                args: vec!["server.js".to_string()],
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let config = manager.get_config().await.unwrap();
    assert!(config.mcp_servers.contains_key("test-server"));

    manager.remove_server("test-server").await.unwrap();
    let config = manager.get_config().await.unwrap();
    assert!(!config.mcp_servers.contains_key("test-server"));
}

#[tokio::test]
async fn test_list_servers_empty() {
    let manager = McpManager::new(None);
    let servers = manager.list_servers().await;
    assert!(servers.is_empty());
}

#[tokio::test]
async fn test_disconnect_nonexistent() {
    let manager = McpManager::new(None);
    let result = manager.disconnect_server("nonexistent").await;
    assert!(matches!(result, Err(McpError::ServerNotFound(_))));
}

#[tokio::test]
async fn test_remove_failed_server() {
    let manager = McpManager::new(Some(PathBuf::from("/tmp")));
    // Removing a non-existent server should not panic
    manager.remove_failed_server("nonexistent").await;
    assert_eq!(manager.connected_count().await, 0);
}

#[tokio::test]
async fn test_connect_all_skips_disabled_servers() {
    let manager = McpManager::new(Some(PathBuf::from("/tmp")));
    {
        let mut config = manager.config.write().await;
        let mut cfg = McpConfig::default();
        cfg.mcp_servers.insert(
            "disabled_server".to_string(),
            McpServerConfig {
                enabled: false,
                auto_start: true,
                ..McpServerConfig::default()
            },
        );
        cfg.mcp_servers.insert(
            "non_autostart".to_string(),
            McpServerConfig {
                enabled: true,
                auto_start: false,
                ..McpServerConfig::default()
            },
        );
        *config = Some(cfg);
    }

    let connected = manager.connect_all().await.unwrap();
    assert!(connected.is_empty());
}

#[tokio::test]
async fn test_disconnect_all_empty() {
    let manager = McpManager::new(None);
    let result = manager.disconnect_all().await;
    assert!(result.is_ok());
    assert_eq!(manager.connected_count().await, 0);
}

#[tokio::test]
async fn test_is_connected_returns_false_for_unknown() {
    let manager = McpManager::new(None);
    assert!(!manager.is_connected("unknown").await);
}

#[tokio::test]
async fn test_connected_count_starts_at_zero() {
    let manager = McpManager::new(None);
    assert_eq!(manager.connected_count().await, 0);
}

#[tokio::test]
async fn test_remove_server_cleans_up_health_and_cache() {
    let manager = McpManager::new(Some(PathBuf::from("/tmp")));

    {
        let mut config = manager.config.write().await;
        let mut mcp_config = McpConfig::default();
        mcp_config.mcp_servers.insert(
            "cleanup-test".to_string(),
            McpServerConfig {
                command: "node".to_string(),
                args: vec!["test.js".to_string()],
                ..Default::default()
            },
        );
        *config = Some(mcp_config);
    }

    {
        let mut states = manager.health_states.write().await;
        states.insert("cleanup-test".to_string(), ServerHealthState::default());
    }
    {
        let mut cache = manager.tool_schema_cache.write().await;
        cache.insert(
            "cleanup-test".to_string(),
            super::super::ToolSchemaCache {
                tools: vec![],
                invalidated: false,
            },
        );
    }

    manager.remove_server("cleanup-test").await.unwrap();

    assert!(manager.get_health_state("cleanup-test").await.is_none());
    let cache = manager.tool_schema_cache.read().await;
    assert!(cache.get("cleanup-test").is_none());
}

/// Integration test: connect to a mock MCP server, run initialize handshake,
/// and discover tools.
#[tokio::test]
async fn test_full_lifecycle_with_mock_server() {
    use super::super::ServerHealthStatus;
    use crate::config::TransportType;

    let script = r#"
import sys, json

def read_message():
while True:
    line = sys.stdin.readline()
    if not line:
        return None
    if line.startswith("Content-Length:"):
        length = int(line.split(":")[1].strip())
        sys.stdin.readline()  # blank line
        body = sys.stdin.read(length)
        return json.loads(body)

def write_message(obj):
body = json.dumps(obj)
sys.stdout.write(f"Content-Length: {len(body)}\r\n\r\n{body}")
sys.stdout.flush()

while True:
msg = read_message()
if msg is None:
    break
if "id" not in msg:
    continue  # notification, no response
method = msg.get("method", "")
if method == "initialize":
    write_message({
        "jsonrpc": "2.0",
        "id": msg["id"],
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "mock-server", "version": "0.1.0"}
        }
    })
elif method == "tools/list":
    write_message({
        "jsonrpc": "2.0",
        "id": msg["id"],
        "result": {
            "tools": [
                {
                    "name": "greet",
                    "description": "Say hello",
                    "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}}
                }
            ]
        }
    })
elif method == "tools/call":
    name = msg.get("params", {}).get("arguments", {}).get("name", "world")
    write_message({
        "jsonrpc": "2.0",
        "id": msg["id"],
        "result": {
            "content": [{"type": "text", "text": f"Hello, {name}!"}],
            "isError": False
        }
    })
elif method == "ping":
    write_message({
        "jsonrpc": "2.0",
        "id": msg["id"],
        "result": {}
    })
else:
    write_message({
        "jsonrpc": "2.0",
        "id": msg["id"],
        "error": {"code": -32601, "message": "Method not found"}
    })
"#;

    let manager = McpManager::new(Some(PathBuf::from("/tmp")));

    {
        let mut config = manager.config.write().await;
        let mut mcp_config = McpConfig::default();
        mcp_config.mcp_servers.insert(
            "mock".to_string(),
            McpServerConfig {
                command: "python3".to_string(),
                args: vec!["-c".to_string(), script.to_string()],
                transport: TransportType::Stdio,
                enabled: true,
                auto_start: true,
                ..Default::default()
            },
        );
        *config = Some(mcp_config);
    }

    manager.connect_server("mock").await.unwrap();
    assert!(manager.is_connected("mock").await);
    assert_eq!(manager.connected_count().await, 1);

    let schemas = manager.get_all_tool_schemas().await;
    assert_eq!(schemas.len(), 1);
    assert_eq!(schemas[0].name, "mock__greet");
    assert_eq!(schemas[0].original_name, "greet");
    assert_eq!(schemas[0].description, "Say hello");

    let result = manager
        .call_tool("mock", "greet", serde_json::json!({"name": "Rust"}))
        .await
        .unwrap();
    assert!(!result.is_error);
    assert_eq!(result.content.len(), 1);

    let servers = manager.list_servers().await;
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "mock");
    assert_eq!(servers[0].tools.len(), 1);

    let ping_ok = manager.ping_server("mock").await;
    assert!(ping_ok);

    let health = manager.get_health_state("mock").await.unwrap();
    assert_eq!(health.status, ServerHealthStatus::Healthy);
    assert_eq!(health.consecutive_failures, 0);

    manager.disconnect_server("mock").await.unwrap();
    assert!(!manager.is_connected("mock").await);
    assert_eq!(manager.connected_count().await, 0);
}
