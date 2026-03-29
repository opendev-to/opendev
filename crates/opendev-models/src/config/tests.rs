use super::*;

#[test]
fn test_default_config() {
    let config = AppConfig::default();
    assert_eq!(config.model_provider, "fireworks");
    assert_eq!(config.temperature, 0.6);
    assert_eq!(config.max_tokens, 16384);
    assert!(config.enable_bash);
}

#[test]
fn test_config_roundtrip() {
    let config = AppConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: AppConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.model_provider, config.model_provider);
    assert_eq!(deserialized.model, config.model);
}

#[test]
fn test_scoring_weights_validation() {
    let valid = PlaybookScoringWeights::default();
    assert!(valid.validate().is_ok());

    let invalid = PlaybookScoringWeights {
        effectiveness: 1.5,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn test_partial_config_deserialization() {
    // Should fill in defaults for missing fields
    let json = r#"{"model_provider": "openai", "model": "gpt-4"}"#;
    let config: AppConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.model_provider, "openai");
    assert_eq!(config.model, "gpt-4");
    assert_eq!(config.temperature, 0.6); // default
    assert!(config.enable_bash); // default
}

#[test]
fn test_get_api_key_prefers_env_over_config() {
    let env_name = "OPENAI_API_KEY";
    let old = std::env::var(env_name).ok();
    unsafe {
        std::env::set_var(env_name, "env-openai-key");
    }

    let config = AppConfig {
        model_provider: "openai".to_string(),
        api_key: Some("config-openai-key".to_string()),
        ..AppConfig::default()
    };

    assert_eq!(config.get_api_key().unwrap(), "env-openai-key");

    match old {
        Some(value) => unsafe { std::env::set_var(env_name, value) },
        None => unsafe { std::env::remove_var(env_name) },
    }
}

#[test]
fn test_get_api_key_custom_provider_prefers_config_key() {
    // Unknown provider with config key → prefer config key (explicitly configured)
    let config = AppConfig {
        model_provider: "cloudflare".to_string(),
        api_key: Some("config-custom-key".to_string()),
        ..AppConfig::default()
    };
    assert_eq!(config.get_api_key().unwrap(), "config-custom-key");
}

#[test]
fn test_get_api_key_custom_provider_openai_env_fallback() {
    // Unknown provider without config key → falls back to OPENAI_API_KEY
    // (only run assertion if OPENAI_API_KEY is actually set to avoid flaky test)
    let config_no_key = AppConfig {
        model_provider: "cloudflare".to_string(),
        api_key: None,
        ..AppConfig::default()
    };
    if std::env::var("OPENAI_API_KEY").is_ok() {
        assert!(config_no_key.get_api_key().is_ok());
    } else {
        assert!(config_no_key.get_api_key().is_err());
    }
}

#[test]
fn test_get_api_key_with_env_prefers_registry() {
    // Registry env var takes priority over everything else
    let config = AppConfig {
        model_provider: "some-unknown-provider".to_string(),
        api_key: Some("config-key".to_string()),
        ..AppConfig::default()
    };

    // With a unique registry env var that IS set
    let env_name = "OPENDEV_TEST_REGISTRY_KEY_7391";
    unsafe { std::env::set_var(env_name, "registry-key") };
    assert_eq!(
        config.get_api_key_with_env(Some(env_name)).unwrap(),
        "registry-key"
    );
    unsafe { std::env::remove_var(env_name) };

    // With registry env var NOT set → falls back to config key
    assert_eq!(
        config
            .get_api_key_with_env(Some("NONEXISTENT_VAR_XYZ"))
            .unwrap(),
        "config-key"
    );

    // With no registry info at all → same as get_api_key()
    assert_eq!(config.get_api_key_with_env(None).unwrap(), "config-key");
}

#[test]
fn test_builtin_env_var_mapping() {
    assert_eq!(AppConfig::builtin_env_var("openai"), "OPENAI_API_KEY");
    assert_eq!(AppConfig::builtin_env_var("anthropic"), "ANTHROPIC_API_KEY");
    assert_eq!(AppConfig::builtin_env_var("deepseek"), "DEEPSEEK_API_KEY");
    assert_eq!(
        AppConfig::builtin_env_var("fireworks-ai"),
        "FIREWORKS_API_KEY"
    );
    assert_eq!(AppConfig::builtin_env_var("xai"), "XAI_API_KEY");
    assert_eq!(AppConfig::builtin_env_var("unknown-provider"), "");
}

#[test]
fn test_convention_env_var() {
    assert_eq!(AppConfig::convention_env_var("zai"), "ZAI_API_KEY");
    assert_eq!(
        AppConfig::convention_env_var("zai-coding-plan"),
        "ZAI_API_KEY"
    );
    assert_eq!(
        AppConfig::convention_env_var("siliconflow-cn"),
        "SILICONFLOW_API_KEY"
    );
    assert_eq!(
        AppConfig::convention_env_var("siliconflow"),
        "SILICONFLOW_API_KEY"
    );
    assert_eq!(
        AppConfig::convention_env_var("perplexity-agent"),
        "PERPLEXITY_API_KEY"
    );
    assert_eq!(
        AppConfig::convention_env_var("nano-gpt"),
        "NANO_GPT_API_KEY"
    );
    assert_eq!(AppConfig::convention_env_var(""), "");
}
