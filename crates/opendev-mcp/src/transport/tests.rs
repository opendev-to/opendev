use super::*;

#[test]
fn test_create_http_transport() {
    let config = McpServerConfig {
        url: Some("https://example.com/mcp".to_string()),
        transport: TransportType::Http,
        ..Default::default()
    };

    let transport = create_transport(&config).unwrap();
    assert_eq!(transport.transport_type(), "http");
}

#[test]
fn test_create_sse_transport() {
    let config = McpServerConfig {
        url: Some("https://example.com/sse".to_string()),
        transport: TransportType::Sse,
        ..Default::default()
    };

    let transport = create_transport(&config).unwrap();
    assert_eq!(transport.transport_type(), "sse");
}

#[test]
fn test_create_stdio_transport() {
    let config = McpServerConfig {
        command: "npx".to_string(),
        args: vec!["mcp-server-test".to_string()],
        transport: TransportType::Stdio,
        ..Default::default()
    };

    let transport = create_transport(&config).unwrap();
    assert_eq!(transport.transport_type(), "stdio");
}

#[test]
fn test_http_without_url_fails() {
    let config = McpServerConfig {
        transport: TransportType::Http,
        ..Default::default()
    };

    assert!(create_transport(&config).is_err());
}

#[test]
fn test_stdio_without_command_fails() {
    let config = McpServerConfig {
        transport: TransportType::Stdio,
        ..Default::default()
    };

    assert!(create_transport(&config).is_err());
}

#[test]
fn test_npx_without_args_fails() {
    let config = McpServerConfig {
        command: "npx".to_string(),
        args: vec![],
        transport: TransportType::Stdio,
        ..Default::default()
    };

    assert!(create_transport(&config).is_err());
}
