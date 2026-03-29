use super::*;

#[test]
fn test_parse_manifest() {
    let json = r#"{
        "name": "my_tool",
        "description": "A custom tool",
        "command": "./run.sh",
        "parameters": {
            "type": "object",
            "properties": {
                "input": { "type": "string" }
            },
            "required": ["input"]
        },
        "timeout_secs": 60
    }"#;

    let manifest: CustomToolManifest = serde_json::from_str(json).unwrap();
    assert_eq!(manifest.name, "my_tool");
    assert_eq!(manifest.description, "A custom tool");
    assert_eq!(manifest.command, "./run.sh");
    assert_eq!(manifest.timeout_secs, 60);
}

#[test]
fn test_parse_manifest_defaults() {
    let json = r#"{
        "name": "simple",
        "description": "Simple tool",
        "command": "echo"
    }"#;

    let manifest: CustomToolManifest = serde_json::from_str(json).unwrap();
    assert_eq!(manifest.timeout_secs, 30);
    assert!(manifest.parameters.is_object());
}

#[test]
fn test_discover_empty_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let tools = discover_custom_tools(tmp.path());
    assert!(tools.is_empty());
}

#[test]
fn test_discover_finds_manifests() {
    let tmp = tempfile::TempDir::new().unwrap();
    let tool_dir = tmp.path().join(".opendev").join("tools");
    std::fs::create_dir_all(&tool_dir).unwrap();

    // Create a manifest
    let manifest = r#"{
        "name": "test_tool",
        "description": "Test",
        "command": "./test.sh"
    }"#;
    std::fs::write(tool_dir.join("test.tool.json"), manifest).unwrap();

    // Create a non-manifest file (should be ignored)
    std::fs::write(tool_dir.join("readme.md"), "ignore me").unwrap();

    let tools = discover_custom_tools(tmp.path());
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name(), "test_tool");
}

#[test]
fn test_discover_deduplicates() {
    let tmp = tempfile::TempDir::new().unwrap();

    // Same tool name in both directories
    let dir1 = tmp.path().join(".opendev").join("tools");
    let dir2 = tmp.path().join(".opencode").join("tool");
    std::fs::create_dir_all(&dir1).unwrap();
    std::fs::create_dir_all(&dir2).unwrap();

    let manifest = r#"{"name": "dup", "description": "Dup", "command": "./x.sh"}"#;
    std::fs::write(dir1.join("dup.tool.json"), manifest).unwrap();
    std::fs::write(dir2.join("dup.tool.json"), manifest).unwrap();

    let tools = discover_custom_tools(tmp.path());
    assert_eq!(
        tools.len(),
        1,
        "Duplicate tool names should be deduplicated"
    );
}

#[test]
fn test_resolve_command_relative() {
    let manifest = CustomToolManifest {
        name: "t".into(),
        description: "t".into(),
        command: "./run.sh".into(),
        parameters: default_params_schema(),
        timeout_secs: 30,
    };
    let tool = CustomTool::new(manifest, PathBuf::from("/project/.opendev/tools"));
    assert_eq!(
        tool.resolve_command(),
        PathBuf::from("/project/.opendev/tools/run.sh")
    );
}

#[test]
fn test_resolve_command_absolute() {
    let manifest = CustomToolManifest {
        name: "t".into(),
        description: "t".into(),
        command: "/usr/bin/my-tool".into(),
        parameters: default_params_schema(),
        timeout_secs: 30,
    };
    let tool = CustomTool::new(manifest, PathBuf::from("/project/.opendev/tools"));
    assert_eq!(tool.resolve_command(), PathBuf::from("/usr/bin/my-tool"));
}

#[tokio::test]
async fn test_execute_missing_command() {
    let manifest = CustomToolManifest {
        name: "missing".into(),
        description: "Missing".into(),
        command: "./nonexistent.sh".into(),
        parameters: default_params_schema(),
        timeout_secs: 5,
    };
    let tmp = tempfile::TempDir::new().unwrap();
    let tool = CustomTool::new(manifest, tmp.path().to_path_buf());
    let ctx = ToolContext::new(tmp.path());
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[cfg(unix)]
#[tokio::test]
async fn test_execute_simple_command() {
    let tmp = tempfile::TempDir::new().unwrap();
    let script_path = tmp.path().join("echo.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho \"hello from custom tool\"").unwrap();

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let manifest = CustomToolManifest {
        name: "echo_tool".into(),
        description: "Echo".into(),
        command: "./echo.sh".into(),
        parameters: default_params_schema(),
        timeout_secs: 5,
    };
    let tool = CustomTool::new(manifest, tmp.path().to_path_buf());
    let ctx = ToolContext::new(tmp.path());
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(result.success, "Should succeed: {:?}", result.error);
    assert!(result.output.unwrap().contains("hello from custom tool"));
}

#[cfg(unix)]
#[tokio::test]
async fn test_execute_failing_command() {
    let tmp = tempfile::TempDir::new().unwrap();
    let script_path = tmp.path().join("fail.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho 'error msg' >&2\nexit 1").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let manifest = CustomToolManifest {
        name: "fail_tool".into(),
        description: "Fail".into(),
        command: "./fail.sh".into(),
        parameters: default_params_schema(),
        timeout_secs: 5,
    };
    let tool = CustomTool::new(manifest, tmp.path().to_path_buf());
    let ctx = ToolContext::new(tmp.path());
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("error msg"));
}
