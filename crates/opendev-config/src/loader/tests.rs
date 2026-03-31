use super::*;
use tempfile::TempDir;

#[test]
fn test_load_defaults() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.model_provider, "fireworks");
    assert_eq!(config.temperature, 0.6);
}

#[test]
fn test_load_with_global_settings() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(&global, r#"{"model_provider": "openai", "model": "gpt-4"}"#).unwrap();

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.model_provider, "openai");
    assert_eq!(config.model, "gpt-4");
    // Defaults preserved for unset fields
    assert_eq!(config.temperature, 0.6);
}

#[test]
fn test_project_overrides_global() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(&global, r#"{"model_provider": "openai", "model": "gpt-4"}"#).unwrap();
    std::fs::write(&project, r#"{"model": "gpt-4-turbo"}"#).unwrap();

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.model_provider, "openai"); // from global
    assert_eq!(config.model, "gpt-4-turbo"); // overridden by project
}

#[test]
fn test_merge_preserves_defaults() {
    let base = AppConfig::default();
    let overrides = serde_json::json!({"verbose": true});
    let merged = ConfigLoader::merge(base, overrides);
    assert!(merged.verbose);
    assert_eq!(merged.temperature, 0.6); // default preserved
}

#[test]
fn test_save_and_reload() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");

    let mut config = AppConfig::default();
    config.model_provider = "anthropic".to_string();
    config.model = "claude-3-opus".to_string();
    config.verbose = true;

    ConfigLoader::save(&config, &path).unwrap();

    // Reload
    let loaded = ConfigLoader::load(&path, &tmp.path().join("nonexistent.json")).unwrap();
    assert_eq!(loaded.model_provider, "anthropic");
    assert_eq!(loaded.model, "claude-3-opus");
    assert!(loaded.verbose);
    assert_eq!(loaded.temperature, 0.6); // default preserved
}

#[test]
fn test_save_creates_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("nested").join("dir").join("settings.json");

    let config = AppConfig::default();
    ConfigLoader::save(&config, &path).unwrap();

    assert!(path.exists());
}

#[test]
fn test_save_atomic_no_corruption() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");

    // Save twice; second write should not leave tmp file
    let config = AppConfig::default();
    ConfigLoader::save(&config, &path).unwrap();
    ConfigLoader::save(&config, &path).unwrap();

    assert!(path.exists());
    assert!(!tmp.path().join("settings.json.tmp").exists());
}

#[test]
fn test_validate_default_config() {
    let config = AppConfig::default();
    assert!(ConfigLoader::validate(&config).is_ok());
}

#[test]
fn test_validate_empty_model() {
    let mut config = AppConfig::default();
    config.model = String::new();
    let err = ConfigLoader::validate(&config).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("model name must be a non-empty string"));
}

#[test]
fn test_validate_empty_provider() {
    let mut config = AppConfig::default();
    config.model_provider = "  ".to_string();
    let err = ConfigLoader::validate(&config).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("model_provider must be a non-empty string"));
}

#[test]
fn test_validate_empty_api_key() {
    let mut config = AppConfig::default();
    config.api_key = Some(String::new());
    let err = ConfigLoader::validate(&config).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("api_key must be non-empty when set"));
}

#[test]
fn test_validate_temperature_out_of_range() {
    let mut config = AppConfig::default();
    config.temperature = 2.5;
    let err = ConfigLoader::validate(&config).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("temperature must be between 0.0 and 2.0"));

    // Negative temperature
    config.temperature = -0.1;
    let err = ConfigLoader::validate(&config).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("temperature must be between 0.0 and 2.0"));
}

#[test]
fn test_validate_zero_max_tokens() {
    let mut config = AppConfig::default();
    config.max_tokens = 0;
    let err = ConfigLoader::validate(&config).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("max_tokens must be positive"));
}

#[test]
fn test_validate_multiple_errors() {
    let mut config = AppConfig::default();
    config.model = String::new();
    config.temperature = 3.0;
    config.max_tokens = 0;
    let err = ConfigLoader::validate(&config).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("model name"));
    assert!(msg.contains("temperature"));
    assert!(msg.contains("max_tokens"));
}

#[test]
fn test_validate_boundary_temperature() {
    let mut config = AppConfig::default();
    config.temperature = 0.0;
    assert!(ConfigLoader::validate(&config).is_ok());
    config.temperature = 2.0;
    assert!(ConfigLoader::validate(&config).is_ok());
}

// --- Environment variable override tests ---
// Uses apply_env_overrides_with() with a mock lookup to avoid global env var races.

fn mock_env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: std::collections::HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |key| map.get(key).cloned()
}

#[test]
fn test_env_override_model_provider() {
    let mut config = AppConfig::default();
    assert_eq!(config.model_provider, "fireworks");

    ConfigLoader::apply_env_overrides_with(
        &mut config,
        mock_env(&[("OPENDEV_MODEL_PROVIDER", "anthropic")]),
    );
    assert_eq!(config.model_provider, "anthropic");
}

#[test]
fn test_env_override_model() {
    let mut config = AppConfig::default();
    ConfigLoader::apply_env_overrides_with(&mut config, mock_env(&[("OPENDEV_MODEL", "gpt-4o")]));
    assert_eq!(config.model, "gpt-4o");
}

#[test]
fn test_env_override_max_tokens() {
    let mut config = AppConfig::default();
    ConfigLoader::apply_env_overrides_with(
        &mut config,
        mock_env(&[("OPENDEV_MAX_TOKENS", "8192")]),
    );
    assert_eq!(config.max_tokens, 8192);
}

#[test]
fn test_env_override_max_tokens_invalid_ignored() {
    let mut config = AppConfig::default();
    config.max_tokens = 99999;
    ConfigLoader::apply_env_overrides_with(
        &mut config,
        mock_env(&[("OPENDEV_MAX_TOKENS", "not_a_number")]),
    );
    assert_eq!(config.max_tokens, 99999);
}

#[test]
fn test_env_override_temperature() {
    let mut config = AppConfig::default();
    ConfigLoader::apply_env_overrides_with(
        &mut config,
        mock_env(&[("OPENDEV_TEMPERATURE", "0.9")]),
    );
    assert!((config.temperature - 0.9).abs() < f64::EPSILON);
}

#[test]
fn test_env_override_temperature_invalid_ignored() {
    let mut config = AppConfig::default();
    config.temperature = 1.234;
    ConfigLoader::apply_env_overrides_with(
        &mut config,
        mock_env(&[("OPENDEV_TEMPERATURE", "hot")]),
    );
    assert!((config.temperature - 1.234).abs() < f64::EPSILON);
}

#[test]
fn test_env_override_verbose() {
    let mut config = AppConfig::default();
    assert!(!config.verbose);

    ConfigLoader::apply_env_overrides_with(&mut config, mock_env(&[("OPENDEV_VERBOSE", "true")]));
    assert!(config.verbose);

    config.verbose = false;
    ConfigLoader::apply_env_overrides_with(&mut config, mock_env(&[("OPENDEV_VERBOSE", "1")]));
    assert!(config.verbose);

    config.verbose = true;
    ConfigLoader::apply_env_overrides_with(&mut config, mock_env(&[("OPENDEV_VERBOSE", "false")]));
    assert!(!config.verbose);
}

#[test]
fn test_env_override_debug() {
    let mut config = AppConfig::default();
    assert!(config.debug_logging); // default is now true

    // Env override can still toggle it off
    ConfigLoader::apply_env_overrides_with(&mut config, mock_env(&[("OPENDEV_DEBUG", "0")]));
    assert!(!config.debug_logging);

    // And back on
    ConfigLoader::apply_env_overrides_with(&mut config, mock_env(&[("OPENDEV_DEBUG", "TRUE")]));
    assert!(config.debug_logging);
}

#[test]
fn test_env_override_missing_vars_noop() {
    let mut config = AppConfig::default();
    let original = config.clone();
    // Empty map = no env vars set
    ConfigLoader::apply_env_overrides_with(&mut config, mock_env(&[]));
    assert_eq!(config.model_provider, original.model_provider);
    assert_eq!(config.model, original.model);
    assert_eq!(config.max_tokens, original.max_tokens);
    assert!((config.temperature - original.temperature).abs() < f64::EPSILON);
}

// --- Unknown field detection tests ---

#[test]
fn test_warn_unknown_fields_no_unknowns() {
    let json = serde_json::json!({"model": "gpt-4", "temperature": 0.7});
    let unknown = ConfigLoader::warn_unknown_fields(&json, "test");
    assert!(unknown.is_empty());
}

#[test]
fn test_warn_unknown_fields_detects_typos() {
    let json = serde_json::json!({
        "modle": "gpt-4",
        "temperatre": 0.7,
        "model_provider": "openai"
    });
    let unknown = ConfigLoader::warn_unknown_fields(&json, "test");
    assert_eq!(unknown.len(), 2);
    assert!(unknown.contains(&"modle".to_string()));
    assert!(unknown.contains(&"temperatre".to_string()));
}

#[test]
fn test_warn_unknown_fields_completely_unknown() {
    let json = serde_json::json!({"xyzzy_bogus_field": true});
    let unknown = ConfigLoader::warn_unknown_fields(&json, "test");
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0], "xyzzy_bogus_field");
}

#[test]
fn test_warn_unknown_fields_non_object() {
    // Non-object JSON should return empty (no keys to check)
    let json = serde_json::json!("not an object");
    let unknown = ConfigLoader::warn_unknown_fields(&json, "test");
    assert!(unknown.is_empty());
}

#[test]
fn test_closest_field_exact() {
    let known = ConfigLoader::known_field_names();
    // "model" should match itself exactly (distance 0)
    let closest = ConfigLoader::closest_field("model", &known);
    assert_eq!(closest, Some("model"));
}

#[test]
fn test_closest_field_typo() {
    let known = ConfigLoader::known_field_names();
    // "modle" -> "model" (distance 1: transposition)
    let closest = ConfigLoader::closest_field("modle", &known);
    assert_eq!(closest, Some("model"));
}

#[test]
fn test_closest_field_no_match() {
    let known = ConfigLoader::known_field_names();
    // Completely unrelated string should return None
    let closest = ConfigLoader::closest_field("xyzzy_bogus", &known);
    assert!(closest.is_none());
}

#[test]
fn test_edit_distance() {
    assert_eq!(ConfigLoader::edit_distance("model", "model"), 0);
    assert_eq!(ConfigLoader::edit_distance("modle", "model"), 2);
    assert_eq!(ConfigLoader::edit_distance("", "abc"), 3);
    assert_eq!(ConfigLoader::edit_distance("abc", ""), 3);
    assert_eq!(ConfigLoader::edit_distance("kitten", "sitting"), 3);
}

#[test]
fn test_known_field_names_complete() {
    // Verify that all fields from a serialized default config are in the known set
    let config = AppConfig::default();
    let json = serde_json::to_value(&config).unwrap();
    let known = ConfigLoader::known_field_names();
    if let Some(obj) = json.as_object() {
        for key in obj.keys() {
            assert!(
                known.contains(key.as_str()),
                "Field '{}' from AppConfig is missing from known_field_names()",
                key
            );
        }
    }
}

#[test]
fn test_load_with_unknown_fields_still_works() {
    // Unknown fields should produce warnings but not prevent loading
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(
        &global,
        r#"{"model_provider": "openai", "modle": "gpt-4", "unknown_thing": true}"#,
    )
    .unwrap();

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.model_provider, "openai");
    // "modle" was ignored, default model used
    assert_ne!(config.model, "gpt-4");
}

// --- Cross-field validation tests ---

#[test]
fn test_cross_field_default_config_no_warnings() {
    let config = AppConfig::default();
    let warnings = ConfigLoader::validate_cross_field(&config);
    assert!(
        warnings.is_empty(),
        "Default config should have no warnings, got: {:?}",
        warnings
    );
}
#[test]
fn test_cross_field_zero_explore_agents() {
    let mut config = AppConfig::default();
    config.plan_mode_explore_agent_count = 0;
    let warnings = ConfigLoader::validate_cross_field(&config);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("plan_mode_explore_agent_count")),
        "Should warn about 0 explore agents: {:?}",
        warnings
    );
}

#[test]
fn test_cross_field_auto_mode_no_bash() {
    let mut config = AppConfig::default();
    config.auto_mode.enabled = true;
    config.enable_bash = false;
    let warnings = ConfigLoader::validate_cross_field(&config);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("auto_mode") && w.contains("enable_bash")),
        "Should warn about auto mode without bash: {:?}",
        warnings
    );
}

#[test]
fn test_cross_field_model_variant_empty_model() {
    let mut config = AppConfig::default();
    config.model_variants.insert(
        "bad".to_string(),
        opendev_models::ModelVariant {
            name: "bad".to_string(),
            model: "".to_string(),
            provider: "openai".to_string(),
            temperature: 0.6,
            max_tokens: 4096,
            description: String::new(),
        },
    );
    let warnings = ConfigLoader::validate_cross_field(&config);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("model_variants") && w.contains("model is empty")),
        "Should warn about empty model in variant: {:?}",
        warnings
    );
}

#[test]
fn test_cross_field_model_variant_bad_temperature() {
    let mut config = AppConfig::default();
    config.model_variants.insert(
        "hot".to_string(),
        opendev_models::ModelVariant {
            name: "hot".to_string(),
            model: "gpt-4".to_string(),
            provider: "openai".to_string(),
            temperature: 5.0,
            max_tokens: 4096,
            description: String::new(),
        },
    );
    let warnings = ConfigLoader::validate_cross_field(&config);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("temperature") && w.contains("hot")),
        "Should warn about bad variant temperature: {:?}",
        warnings
    );
}

#[test]
fn test_cross_field_low_context_tokens() {
    let mut config = AppConfig::default();
    config.max_context_tokens = 500;
    let warnings = ConfigLoader::validate_cross_field(&config);
    assert!(
        warnings.iter().any(|w| w.contains("max_context_tokens")),
        "Should warn about low context tokens: {:?}",
        warnings
    );
}

#[test]
fn test_cross_field_zero_bash_timeout() {
    let mut config = AppConfig::default();
    config.bash_timeout = 0;
    let warnings = ConfigLoader::validate_cross_field(&config);
    assert!(
        warnings.iter().any(|w| w.contains("bash_timeout")),
        "Should warn about zero bash timeout: {:?}",
        warnings
    );
}

#[test]
fn test_cross_field_multiple_warnings() {
    let mut config = AppConfig::default();
    config.bash_timeout = 0;
    config.max_context_tokens = 100;
    let warnings = ConfigLoader::validate_cross_field(&config);
    assert!(
        warnings.len() >= 2,
        "Should have multiple warnings: {:?}",
        warnings
    );
}

// --- Template substitution tests ---

#[test]
fn test_substitute_env_variable() {
    unsafe { std::env::set_var("OPENDEV_TEST_SUB_VAR", "test-value-123") };
    let input = r#"{"api_key": "{env:OPENDEV_TEST_SUB_VAR}"}"#;
    let result = ConfigLoader::substitute_templates(input, Path::new("."));
    assert_eq!(result, r#"{"api_key": "test-value-123"}"#);
    unsafe { std::env::remove_var("OPENDEV_TEST_SUB_VAR") };
}

#[test]
fn test_substitute_env_missing_variable() {
    let input = r#"{"key": "{env:OPENDEV_NONEXISTENT_9999}"}"#;
    let result = ConfigLoader::substitute_templates(input, Path::new("."));
    assert_eq!(result, r#"{"key": ""}"#);
}

#[test]
fn test_substitute_file_inclusion() {
    let tmp = TempDir::new().unwrap();
    let secret_file = tmp.path().join("secret.txt");
    std::fs::write(&secret_file, "my-secret-key\n").unwrap();

    let input = format!(r#"{{"api_key": "{{file:{}}}"}}"#, secret_file.display());
    let result = ConfigLoader::substitute_templates(&input, Path::new("."));
    assert_eq!(result, r#"{"api_key": "my-secret-key"}"#);
}

#[test]
fn test_substitute_file_relative_path() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("key.txt"), "relative-key").unwrap();

    let input = r#"{"api_key": "{file:key.txt}"}"#;
    let result = ConfigLoader::substitute_templates(input, tmp.path());
    assert_eq!(result, r#"{"api_key": "relative-key"}"#);
}

#[test]
fn test_substitute_file_missing() {
    let input = r#"{"api_key": "{file:/nonexistent/path/to/file.txt}"}"#;
    let result = ConfigLoader::substitute_templates(input, Path::new("."));
    assert_eq!(result, r#"{"api_key": ""}"#);
}

#[test]
fn test_substitute_multiple_tokens() {
    unsafe { std::env::set_var("OPENDEV_TEST_MULTI_A", "val-a") };
    unsafe { std::env::set_var("OPENDEV_TEST_MULTI_B", "val-b") };
    let input = r#"{"a": "{env:OPENDEV_TEST_MULTI_A}", "b": "{env:OPENDEV_TEST_MULTI_B}"}"#;
    let result = ConfigLoader::substitute_templates(input, Path::new("."));
    assert_eq!(result, r#"{"a": "val-a", "b": "val-b"}"#);
    unsafe { std::env::remove_var("OPENDEV_TEST_MULTI_A") };
    unsafe { std::env::remove_var("OPENDEV_TEST_MULTI_B") };
}

#[test]
fn test_substitute_no_tokens() {
    let input = r#"{"model": "gpt-4", "temperature": 0.7}"#;
    let result = ConfigLoader::substitute_templates(input, Path::new("."));
    assert_eq!(result, input);
}

#[test]
fn test_substitute_file_with_special_chars() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("special.txt");
    std::fs::write(&file, "value with \"quotes\" and\nnewlines").unwrap();

    let input = format!(r#"{{"data": "{{file:{}}}"}}"#, file.display());
    let result = ConfigLoader::substitute_templates(&input, Path::new("."));
    // Should be JSON-escaped
    assert!(result.contains(r#"value with \"quotes\" and\nnewlines"#));
}

#[test]
fn test_substitute_env_then_load() {
    let tmp = TempDir::new().unwrap();
    unsafe { std::env::set_var("OPENDEV_TEST_PROVIDER_SUB", "anthropic") };

    let config_file = tmp.path().join("config.json");
    std::fs::write(
        &config_file,
        r#"{"model_provider": "{env:OPENDEV_TEST_PROVIDER_SUB}"}"#,
    )
    .unwrap();

    let empty = tmp.path().join("empty.json");
    let config = ConfigLoader::load(&config_file, &empty).unwrap();
    assert_eq!(config.model_provider, "anthropic");

    unsafe { std::env::remove_var("OPENDEV_TEST_PROVIDER_SUB") };
}
