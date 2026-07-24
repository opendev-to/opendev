use super::*;

#[tokio::test]
async fn test_runtime_creation() {
    let tmp = tempfile::tempdir().unwrap();
    let session_dir = tmp.path().join("sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let sm = SessionManager::new(session_dir).unwrap();
    let config = AppConfig::default();

    let runtime = AgentRuntime::new(config, tmp.path(), sm).await;
    assert!(runtime.is_ok());
    let rt = runtime.unwrap();
    // Should have tools registered
    assert!(rt.tool_registry.tool_names().len() > 10);
    assert!(
        !rt.tool_registry
            .tool_names()
            .contains(&"batch_tool".to_string()),
        "batch_tool should not be registered"
    );
    assert!(
        !rt.tool_registry
            .get_schemas()
            .iter()
            .any(|schema: &serde_json::Value| schema
                .get("function")
                .and_then(|f: &serde_json::Value| f.get("name"))
                .and_then(|n: &serde_json::Value| n.as_str())
                == Some("batch_tool")),
        "batch_tool schema should not be exposed"
    );
}

#[tokio::test]
async fn test_runtime_debug_format() {
    let tmp = tempfile::tempdir().unwrap();
    let session_dir = tmp.path().join("sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let sm = SessionManager::new(session_dir).unwrap();
    let config = AppConfig::default();

    let runtime = AgentRuntime::new(config, tmp.path(), sm).await.unwrap();
    let debug = format!("{:?}", runtime);
    assert!(debug.contains("AgentRuntime"));
}

#[test]
fn test_resolve_switch_credentials_keyless_builtin_fallback() {
    // Empty registry (offline, cold cache) + keyless local provider:
    // must fall back to builtin defaults and allow an empty API key.
    let registry = opendev_config::ModelRegistry::new();
    assert!(registry.providers.is_empty());
    let config = AppConfig {
        model_provider: "ollama".to_string(),
        api_key: None,
        ..AppConfig::default()
    };

    let (_, base_url) = resolve_switch_credentials(&registry, "ollama", &config)
        .expect("keyless provider must not require an API key");
    assert_eq!(base_url.as_deref(), Some("http://localhost:11434"));

    let (_, base_url) = resolve_switch_credentials(&registry, "lmstudio", &config)
        .expect("keyless provider must not require an API key");
    assert_eq!(base_url.as_deref(), Some("http://localhost:1234"));
}

#[test]
fn test_resolve_switch_credentials_missing_key_errors() {
    // Provider that declares an API key env var which is not set, with no
    // key stored in config: must error with the env var hint.
    let mut registry = opendev_config::ModelRegistry::new();
    registry.providers.insert(
        "needskey".to_string(),
        opendev_config::models_dev::ProviderInfo {
            id: "needskey".to_string(),
            name: "Needs Key".to_string(),
            description: String::new(),
            api_key_env: "OPENDEV_TEST_UNSET_KEY_XYZ".to_string(),
            api_base_url: "https://api.needskey.example".to_string(),
            models: std::collections::HashMap::new(),
        },
    );
    let config = AppConfig {
        model_provider: "needskey".to_string(),
        api_key: None,
        ..AppConfig::default()
    };

    let err = resolve_switch_credentials(&registry, "needskey", &config)
        .expect_err("missing key for keyed provider must error");
    assert!(
        err.contains("OPENDEV_TEST_UNSET_KEY_XYZ"),
        "hint missing: {err}"
    );
}

#[test]
fn test_resolve_switch_credentials_prefers_registry_base_url() {
    // Registry entry overrides the builtin default for the same id.
    let mut registry = opendev_config::ModelRegistry::new();
    registry.providers.insert(
        "ollama".to_string(),
        opendev_config::models_dev::ProviderInfo {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            description: String::new(),
            api_key_env: String::new(),
            api_base_url: "http://10.0.0.5:11434".to_string(),
            models: std::collections::HashMap::new(),
        },
    );
    let config = AppConfig {
        model_provider: "ollama".to_string(),
        api_key: None,
        ..AppConfig::default()
    };

    let (_, base_url) = resolve_switch_credentials(&registry, "ollama", &config).unwrap();
    assert_eq!(base_url.as_deref(), Some("http://10.0.0.5:11434"));
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
