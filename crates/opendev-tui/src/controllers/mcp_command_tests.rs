use super::*;

fn sample_servers() -> Vec<McpServerInfo> {
    vec![
        McpServerInfo {
            name: "sqlite".into(),
            command: "uvx mcp-server-sqlite".into(),
            enabled: true,
        },
        McpServerInfo {
            name: "fs".into(),
            command: "uvx mcp-server-filesystem".into(),
            enabled: false,
        },
    ]
}

#[test]
fn test_list_servers() {
    let mut ctrl = McpCommandController::new(sample_servers());
    let result = ctrl.handle_command("list");
    assert!(result.contains("sqlite"));
    assert!(result.contains("enabled"));
    assert!(result.contains("fs"));
    assert!(result.contains("disabled"));
}

#[test]
fn test_list_empty() {
    let mut ctrl = McpCommandController::new(vec![]);
    let result = ctrl.handle_command("list");
    assert!(result.contains("No MCP servers"));
}

#[test]
fn test_add_server() {
    let mut ctrl = McpCommandController::new(vec![]);
    let result = ctrl.handle_command("add myserver uvx my-mcp-server");
    assert!(result.contains("Added"));
    assert_eq!(ctrl.servers().len(), 1);
    assert_eq!(ctrl.servers()[0].name, "myserver");
    assert!(ctrl.servers()[0].enabled);
}

#[test]
fn test_add_duplicate() {
    let mut ctrl = McpCommandController::new(sample_servers());
    let result = ctrl.handle_command("add sqlite uvx something");
    assert!(result.contains("already exists"));
}

#[test]
fn test_remove_server() {
    let mut ctrl = McpCommandController::new(sample_servers());
    let result = ctrl.handle_command("remove sqlite");
    assert!(result.contains("Removed"));
    assert_eq!(ctrl.servers().len(), 1);
}

#[test]
fn test_remove_not_found() {
    let mut ctrl = McpCommandController::new(sample_servers());
    let result = ctrl.handle_command("remove nonexistent");
    assert!(result.contains("not found"));
}

#[test]
fn test_enable_disable() {
    let mut ctrl = McpCommandController::new(sample_servers());
    let result = ctrl.handle_command("enable fs");
    assert!(result.contains("Enabled"));
    assert!(ctrl.servers()[1].enabled);

    let result = ctrl.handle_command("disable fs");
    assert!(result.contains("Disabled"));
    assert!(!ctrl.servers()[1].enabled);
}

#[test]
fn test_unknown_subcommand() {
    let mut ctrl = McpCommandController::new(vec![]);
    let result = ctrl.handle_command("foobar");
    assert!(result.contains("Unknown"));
}

#[test]
fn test_default_lists() {
    let mut ctrl = McpCommandController::new(sample_servers());
    let result = ctrl.handle_command("");
    assert!(result.contains("MCP Servers"));
}
