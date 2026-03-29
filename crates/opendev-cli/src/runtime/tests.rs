use super::*;

#[test]
fn test_runtime_creation() {
    let tmp = tempfile::tempdir().unwrap();
    let session_dir = tmp.path().join("sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let sm = SessionManager::new(session_dir).unwrap();
    let config = AppConfig::default();

    let runtime = AgentRuntime::new(config, tmp.path(), sm);
    assert!(runtime.is_ok());
    let rt = runtime.unwrap();
    // Should have tools registered
    assert!(rt.tool_registry.tool_names().len() > 20);
    assert!(
        !rt.tool_registry
            .tool_names()
            .contains(&"batch_tool".to_string()),
        "batch_tool should not be registered"
    );
    assert!(
        !rt.tool_registry.get_schemas().iter().any(|schema| schema
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str())
            == Some("batch_tool")),
        "batch_tool schema should not be exposed"
    );
}

#[test]
fn test_runtime_debug_format() {
    let tmp = tempfile::tempdir().unwrap();
    let session_dir = tmp.path().join("sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let sm = SessionManager::new(session_dir).unwrap();
    let config = AppConfig::default();

    let runtime = AgentRuntime::new(config, tmp.path(), sm).unwrap();
    let debug = format!("{:?}", runtime);
    assert!(debug.contains("AgentRuntime"));
}

#[test]
fn test_build_system_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    let prompt = build_system_prompt(tmp.path(), &config);
    // Should produce a non-trivial prompt from embedded templates
    assert!(!prompt.is_empty());
    assert!(
        !prompt.contains("batch_tool"),
        "system prompt should not advertise batch_tool"
    );
}
