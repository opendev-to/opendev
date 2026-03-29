use super::*;

#[test]
fn test_default_server_config() {
    let config = McpServerConfig::default();
    assert!(config.enabled);
    assert!(config.auto_start);
    assert_eq!(config.transport, TransportType::Stdio);
    assert!(config.command.is_empty());
}

#[test]
fn test_config_roundtrip() {
    let mut config = McpConfig::default();
    config.mcp_servers.insert(
        "test-server".to_string(),
        McpServerConfig {
            command: "npx".to_string(),
            args: vec!["mcp-server-test".to_string()],
            ..Default::default()
        },
    );

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: McpConfig = serde_json::from_str(&json).unwrap();
    assert!(deserialized.mcp_servers.contains_key("test-server"));
    assert_eq!(deserialized.mcp_servers["test-server"].command, "npx");
}

#[test]
fn test_config_alias_deserialization() {
    let json = r#"{"mcpServers": {"my-server": {"command": "node", "args": ["server.js"]}}}"#;
    let config: McpConfig = serde_json::from_str(json).unwrap();
    assert!(config.mcp_servers.contains_key("my-server"));
    assert_eq!(config.mcp_servers["my-server"].command, "node");
}

#[test]
fn test_merge_configs() {
    let mut global = McpConfig::default();
    global.mcp_servers.insert(
        "global-server".to_string(),
        McpServerConfig {
            command: "node".to_string(),
            ..Default::default()
        },
    );
    global.mcp_servers.insert(
        "shared".to_string(),
        McpServerConfig {
            command: "old".to_string(),
            ..Default::default()
        },
    );

    let mut project = McpConfig::default();
    project.mcp_servers.insert(
        "project-server".to_string(),
        McpServerConfig {
            command: "python".to_string(),
            ..Default::default()
        },
    );
    project.mcp_servers.insert(
        "shared".to_string(),
        McpServerConfig {
            command: "new".to_string(),
            ..Default::default()
        },
    );

    let merged = merge_configs(&global, Some(&project));
    assert_eq!(merged.mcp_servers.len(), 3);
    assert!(merged.mcp_servers.contains_key("global-server"));
    assert!(merged.mcp_servers.contains_key("project-server"));
    // Project overrides global
    assert_eq!(merged.mcp_servers["shared"].command, "new");
}

#[test]
fn test_expand_env_vars() {
    // SAFETY: test-only; tests in this module are not run concurrently.
    unsafe { std::env::set_var("TEST_MCP_VAR", "hello") };
    assert_eq!(expand_env_vars("${TEST_MCP_VAR}_world"), "hello_world");
    // Unknown variables are left as-is
    assert_eq!(
        expand_env_vars("${UNKNOWN_VAR_12345}"),
        "${UNKNOWN_VAR_12345}"
    );
    // No variables
    assert_eq!(expand_env_vars("no vars here"), "no vars here");
    // SAFETY: test-only cleanup.
    unsafe { std::env::remove_var("TEST_MCP_VAR") };
}

#[test]
fn test_prepare_server_config() {
    // SAFETY: test-only; not run concurrently with env-dependent code.
    unsafe { std::env::set_var("MCP_TEST_TOKEN", "secret123") };
    let config = McpServerConfig {
        command: "node".to_string(),
        args: vec![
            "server.js".to_string(),
            "--token=${MCP_TEST_TOKEN}".to_string(),
        ],
        headers: HashMap::from([(
            "Authorization".to_string(),
            "Bearer ${MCP_TEST_TOKEN}".to_string(),
        )]),
        url: Some("https://example.com/${MCP_TEST_TOKEN}".to_string()),
        ..Default::default()
    };

    let prepared = prepare_server_config(&config);
    assert_eq!(prepared.args[1], "--token=secret123");
    assert_eq!(prepared.headers["Authorization"], "Bearer secret123");
    assert_eq!(
        prepared.url.as_deref(),
        Some("https://example.com/secret123")
    );
    // SAFETY: test-only cleanup.
    unsafe { std::env::remove_var("MCP_TEST_TOKEN") };
}

#[test]
fn test_transport_type_display() {
    assert_eq!(TransportType::Stdio.to_string(), "stdio");
    assert_eq!(TransportType::Sse.to_string(), "sse");
    assert_eq!(TransportType::Http.to_string(), "http");
}

#[test]
fn test_oauth_config_deserialization() {
    let json = r#"{
        "mcpServers": {
            "auth-server": {
                "command": "node",
                "args": ["server.js"],
                "transport": "http",
                "url": "https://mcp.example.com",
                "oauth": {
                    "client_id": "my-client",
                    "client_secret": "my-secret",
                    "token_url": "https://auth.example.com/token",
                    "scope": "mcp:read mcp:write"
                }
            }
        }
    }"#;
    let config: McpConfig = serde_json::from_str(json).unwrap();
    let server = &config.mcp_servers["auth-server"];
    let oauth = server.oauth.as_ref().unwrap();
    assert_eq!(oauth.client_id, "my-client");
    assert_eq!(oauth.client_secret, "my-secret");
    assert_eq!(oauth.token_url, "https://auth.example.com/token");
    assert_eq!(oauth.scope.as_deref(), Some("mcp:read mcp:write"));
}

#[test]
fn test_oauth_config_none_by_default() {
    let config = McpServerConfig::default();
    assert!(config.oauth.is_none());
}

#[test]
fn test_prepare_expands_oauth_env_vars() {
    // SAFETY: test-only; not run concurrently with env-dependent code.
    unsafe { std::env::set_var("MCP_OAUTH_SECRET", "expanded_secret") };
    let config = McpServerConfig {
        oauth: Some(McpOAuthConfig {
            client_id: "client".to_string(),
            client_secret: "${MCP_OAUTH_SECRET}".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scope: Some("read".to_string()),
        }),
        ..Default::default()
    };
    let prepared = prepare_server_config(&config);
    let oauth = prepared.oauth.unwrap();
    assert_eq!(oauth.client_secret, "expanded_secret");
    // SAFETY: test-only cleanup.
    unsafe { std::env::remove_var("MCP_OAUTH_SECRET") };
}

#[test]
fn test_load_config_missing_file() {
    let result = load_config(Path::new("/nonexistent/path/mcp.json"));
    assert!(result.is_ok());
    assert!(result.unwrap().mcp_servers.is_empty());
}

#[test]
fn test_save_and_load_config() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("mcp.json");

    let mut config = McpConfig::default();
    config.mcp_servers.insert(
        "test".to_string(),
        McpServerConfig {
            command: "npx".to_string(),
            args: vec!["mcp-test".to_string()],
            ..Default::default()
        },
    );

    save_config(&config, &config_path).unwrap();
    let loaded = load_config(&config_path).unwrap();
    assert_eq!(loaded.mcp_servers.len(), 1);
    assert_eq!(loaded.mcp_servers["test"].command, "npx");
}

#[test]
fn test_timeout_default() {
    let config = McpServerConfig::default();
    assert!(config.timeout.is_none());
    assert_eq!(config.effective_timeout_ms(), DEFAULT_MCP_TIMEOUT_MS);
}

#[test]
fn test_timeout_custom() {
    let config = McpServerConfig {
        timeout: Some(60_000),
        ..Default::default()
    };
    assert_eq!(config.effective_timeout_ms(), 60_000);
}

#[test]
fn test_timeout_deserialization() {
    let json = r#"{"mcpServers": {"slow-server": {"command": "node", "args": ["server.js"], "timeout": 120000}}}"#;
    let config: McpConfig = serde_json::from_str(json).unwrap();
    let server = &config.mcp_servers["slow-server"];
    assert_eq!(server.timeout, Some(120_000));
    assert_eq!(server.effective_timeout_ms(), 120_000);
}

#[test]
fn test_timeout_not_serialized_when_none() {
    let config = McpServerConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    assert!(!json.contains("timeout"));
}
