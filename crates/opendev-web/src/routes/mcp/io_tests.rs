use super::*;
use tempfile::TempDir;

#[test]
fn test_global_config_path_format() {
    let path = global_config_path();
    assert!(path.ends_with(".opendev/mcp.json"));
}

#[test]
fn test_project_config_path_format() {
    let path = project_config_path("/some/project");
    assert_eq!(path, PathBuf::from("/some/project/.opendev/mcp.json"));
}

#[test]
fn test_load_all_servers_empty() {
    let tmp = TempDir::new().unwrap();
    let servers = load_all_servers(&tmp.path().to_string_lossy());
    // May pick up global config; just verify it doesn't panic.
    let _ = servers;
}

#[test]
fn test_save_and_remove_server() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("mcp.json");

    let config = McpServerConfig {
        command: "echo".to_string(),
        args: vec![],
        env: HashMap::new(),
        enabled: true,
        auto_start: false,
    };

    save_server_to_config("test", &config, &config_path).unwrap();
    assert!(config_path.exists());

    let removed = remove_server_from_config("test", &config_path).unwrap();
    assert!(removed);

    let not_removed = remove_server_from_config("test", &config_path).unwrap();
    assert!(!not_removed);
}
