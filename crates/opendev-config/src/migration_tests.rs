use super::*;

#[test]
fn test_current_version() {
    assert!(CURRENT_CONFIG_VERSION >= 2);
}

#[test]
fn test_migrate_unversioned_config() {
    let value = serde_json::json!({
        "model_provider": "openai",
        "model": "gpt-4"
    });

    assert!(needs_migration(&value));
    assert_eq!(config_version(&value), 0);

    let (migrated, changed) = migrate_config(value);
    assert!(changed);
    assert_eq!(config_version(&migrated), CURRENT_CONFIG_VERSION);

    // Original fields preserved
    assert_eq!(migrated["model_provider"], "openai");
    assert_eq!(migrated["model"], "gpt-4");
}

#[test]
fn test_migrate_already_current() {
    let value = serde_json::json!({
        "config_version": CURRENT_CONFIG_VERSION,
        "model_provider": "anthropic",
        "model": "claude-3-opus"
    });

    assert!(!needs_migration(&value));

    let (migrated, changed) = migrate_config(value);
    assert!(!changed);
    assert_eq!(config_version(&migrated), CURRENT_CONFIG_VERSION);
}

#[test]
fn test_migrate_v0_to_v1() {
    let value = serde_json::json!({
        "model_provider": "fireworks",
        "temperature": 0.7,
        "verbose": true
    });

    let migrated = migrate_v0_to_v1(value);
    assert_eq!(config_version(&migrated), 1);
    assert_eq!(migrated["model_provider"], "fireworks");
    assert_eq!(migrated["temperature"], 0.7);
    assert_eq!(migrated["verbose"], true);
}

#[test]
fn test_migrate_v1_to_v2_with_compact() {
    let value = serde_json::json!({
        "config_version": 1,
        "model_provider": "openai",
        "model": "gpt-4",
        "model_compact": "gemini-2.0-flash",
        "model_compact_provider": "google"
    });

    let (migrated, changed) = migrate_config(value);
    assert!(changed);
    assert_eq!(config_version(&migrated), 2);

    // Flat fields removed
    assert!(migrated.get("model_compact").is_none());
    assert!(migrated.get("model_compact_provider").is_none());

    // Moved into agents.compact
    assert_eq!(migrated["agents"]["compact"]["model"], "gemini-2.0-flash");
    assert_eq!(migrated["agents"]["compact"]["provider"], "google");

    // Original fields preserved
    assert_eq!(migrated["model_provider"], "openai");
    assert_eq!(migrated["model"], "gpt-4");
}

#[test]
fn test_migrate_v1_to_v2_removes_critique() {
    let value = serde_json::json!({
        "config_version": 1,
        "model_critique": "gpt-4o",
        "model_critique_provider": "openai"
    });

    let (migrated, _) = migrate_config(value);
    assert!(migrated.get("model_critique").is_none());
    assert!(migrated.get("model_critique_provider").is_none());
}

#[test]
fn test_migrate_v1_to_v2_no_compact_no_agents() {
    let value = serde_json::json!({
        "config_version": 1,
        "model_provider": "openai",
        "model": "gpt-4"
    });

    let (migrated, changed) = migrate_config(value);
    assert!(changed);
    assert_eq!(config_version(&migrated), 2);
    // No agents entry created when no compact was set
    assert!(migrated.get("agents").is_none());
}

#[test]
fn test_migrate_v1_to_v2_preserves_existing_agents() {
    let value = serde_json::json!({
        "config_version": 1,
        "model_compact": "flash",
        "model_compact_provider": "google",
        "agents": {
            "explore": { "model": "gpt-4o", "max_steps": 5 }
        }
    });

    let (migrated, _) = migrate_config(value);
    // Existing agent preserved
    assert_eq!(migrated["agents"]["explore"]["model"], "gpt-4o");
    assert_eq!(migrated["agents"]["explore"]["max_steps"], 5);
    // Compact added
    assert_eq!(migrated["agents"]["compact"]["model"], "flash");
    assert_eq!(migrated["agents"]["compact"]["provider"], "google");
}

#[test]
fn test_migrate_v1_to_v2_compact_model_only() {
    // Only model set, no provider
    let value = serde_json::json!({
        "config_version": 1,
        "model_compact": "flash"
    });

    let (migrated, _) = migrate_config(value);
    assert_eq!(migrated["agents"]["compact"]["model"], "flash");
    // No provider key
    assert!(migrated["agents"]["compact"].get("provider").is_none());
}

#[test]
fn test_migrate_v0_through_v2() {
    // Full migration from v0 through all versions
    let value = serde_json::json!({
        "model_provider": "openai",
        "model": "gpt-4",
        "model_compact": "flash",
        "model_compact_provider": "google",
        "model_critique": "gpt-4o",
        "model_critique_provider": "openai"
    });

    assert_eq!(config_version(&value), 0);
    let (migrated, changed) = migrate_config(value);
    assert!(changed);
    assert_eq!(config_version(&migrated), 2);

    // Critique removed
    assert!(migrated.get("model_critique").is_none());
    assert!(migrated.get("model_critique_provider").is_none());

    // Compact moved to agents
    assert!(migrated.get("model_compact").is_none());
    assert_eq!(migrated["agents"]["compact"]["model"], "flash");
    assert_eq!(migrated["agents"]["compact"]["provider"], "google");
}

#[test]
fn test_needs_migration_empty_object() {
    let value = serde_json::json!({});
    assert!(needs_migration(&value));
}

#[test]
fn test_config_version_missing() {
    let value = serde_json::json!({"model": "gpt-4"});
    assert_eq!(config_version(&value), 0);
}

#[test]
fn test_config_version_present() {
    let value = serde_json::json!({"config_version": 2});
    assert_eq!(config_version(&value), 2);
}

#[test]
fn test_migrate_preserves_all_fields() {
    let value = serde_json::json!({
        "model_provider": "openai",
        "model": "gpt-4",
        "api_key": "sk-test",
        "max_tokens": 8192,
        "temperature": 0.5,
        "verbose": true,
        "debug_logging": false,
        "custom_field": "should_survive"
    });

    let (migrated, _) = migrate_config(value);
    assert_eq!(migrated["model_provider"], "openai");
    assert_eq!(migrated["api_key"], "sk-test");
    assert_eq!(migrated["max_tokens"], 8192);
    assert_eq!(migrated["custom_field"], "should_survive");
}
