use super::*;
use opendev_config::models_dev::{ModelInfo, ModelRegistry, ProviderInfo};
use std::collections::HashMap;

fn test_registry() -> ModelRegistry {
    let mut providers = HashMap::new();

    let mut openai_models = HashMap::new();
    openai_models.insert(
        "gpt-4o".to_string(),
        ModelInfo {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            provider: "OpenAI".to_string(),
            context_length: 128_000,
            capabilities: vec!["text".to_string(), "vision".to_string()],
            pricing_input: 2.5,
            pricing_output: 10.0,
            pricing_unit: "per million tokens".to_string(),
            serverless: false,
            tunable: false,
            recommended: true,
            max_tokens: Some(16384),
            supports_temperature: true,
            api_type: "chat".to_string(),
        },
    );

    providers.insert(
        "openai".to_string(),
        ProviderInfo {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            description: "GPT-4o, o1, o3 and more".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            api_base_url: "https://api.openai.com/v1".to_string(),
            models: openai_models,
        },
    );

    let mut anthropic_models = HashMap::new();
    anthropic_models.insert(
        "claude-sonnet-4".to_string(),
        ModelInfo {
            id: "claude-sonnet-4-20250514".to_string(),
            name: "Claude Sonnet 4".to_string(),
            provider: "Anthropic".to_string(),
            context_length: 200_000,
            capabilities: vec!["text".to_string(), "reasoning".to_string()],
            pricing_input: 3.0,
            pricing_output: 15.0,
            pricing_unit: "per million tokens".to_string(),
            serverless: false,
            tunable: false,
            recommended: true,
            max_tokens: Some(8192),
            supports_temperature: true,
            api_type: "chat".to_string(),
        },
    );

    providers.insert(
        "anthropic".to_string(),
        ProviderInfo {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            description: "Claude 3.5, Claude 4".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            api_base_url: "https://api.anthropic.com/v1".to_string(),
            models: anthropic_models,
        },
    );

    ModelRegistry { providers }
}

#[test]
fn test_provider_choices() {
    let registry = test_registry();
    let choices = ProviderSetup::provider_choices(&registry);
    assert!(choices.len() >= 2);
    assert!(choices.iter().any(|(id, _, _)| id == "openai"));
    assert!(choices.iter().any(|(id, _, _)| id == "anthropic"));
}

#[test]
fn test_get_provider_config_found() {
    let registry = test_registry();
    let config = ProviderSetup::get_provider_config(&registry, "openai");
    assert!(config.is_some());
    let config = config.unwrap();
    assert_eq!(config.name, "OpenAI");
    assert_eq!(config.env_var, "OPENAI_API_KEY");
    assert_eq!(config.api_format, ApiFormat::OpenAi);
    assert!(!config.models.is_empty());
}

#[test]
fn test_get_provider_config_not_found() {
    let registry = test_registry();
    let config = ProviderSetup::get_provider_config(&registry, "nonexistent");
    assert!(config.is_none());
}

#[test]
fn test_get_provider_config_anthropic() {
    let registry = test_registry();
    let config = ProviderSetup::get_provider_config(&registry, "anthropic").unwrap();
    assert_eq!(config.api_format, ApiFormat::Anthropic);
    assert!(config.api_base_url.contains("anthropic"));
}

#[test]
fn test_get_provider_models() {
    let registry = test_registry();
    let models = ProviderSetup::get_provider_models(&registry, "openai");
    assert!(!models.is_empty());
    assert!(models.iter().any(|(id, _, _)| id == "gpt-4o"));
}

#[test]
fn test_get_provider_models_nonexistent() {
    let registry = test_registry();
    let models = ProviderSetup::get_provider_models(&registry, "nonexistent");
    assert!(models.is_empty());
}

#[test]
fn test_validation_error_display() {
    let e = ValidationError::HttpError("timeout".into());
    assert!(e.to_string().contains("timeout"));

    let e = ValidationError::ApiError {
        status: 401,
        body: "invalid key".into(),
    };
    assert!(e.to_string().contains("401"));

    let e = ValidationError::Unexpected("weird".into());
    assert!(e.to_string().contains("weird"));
}
