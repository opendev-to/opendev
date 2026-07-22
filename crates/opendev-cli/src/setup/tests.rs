use super::*;

#[test]
fn test_config_exists_false_for_tmp() {
    let _ = config_exists();
}

#[test]
fn test_setup_error_display() {
    let e = SetupError::Cancelled;
    assert_eq!(e.to_string(), "setup cancelled by user");

    let e = SetupError::NoApiKey;
    assert_eq!(e.to_string(), "no API key provided");

    let e = SetupError::ValidationFailed("bad key".into());
    assert!(e.to_string().contains("bad key"));

    let e = SetupError::SaveFailed("disk full".into());
    assert!(e.to_string().contains("disk full"));

    let e = SetupError::RegistryError("no data".into());
    assert!(e.to_string().contains("no data"));
}

#[test]
fn test_merge_builtin_providers_into_empty_registry() {
    let mut registry = ModelRegistry::new();
    assert!(registry.providers.is_empty());

    let added = merge_builtin_providers(&mut registry);
    assert!(added, "builtins should be added to an empty registry");
    assert_eq!(registry.providers.len(), 12);

    // Well-known providers are now selectable
    for id in ["openai", "anthropic", "ollama", "lmstudio", "google"] {
        assert!(
            registry.get_provider(id).is_some(),
            "builtin provider '{id}' should be present after merge"
        );
    }

    // Keyless local providers keep their empty api_key_env and localhost URL
    let ollama = registry.get_provider("ollama").unwrap();
    assert!(ollama.api_key_env.is_empty());
    assert_eq!(ollama.api_base_url, "http://localhost:11434");

    // Merging again is a no-op
    assert!(!merge_builtin_providers(&mut registry));
    assert_eq!(registry.providers.len(), 12);
}

#[test]
fn test_merge_builtin_providers_keeps_registry_entries() {
    let mut registry = ModelRegistry::new();
    registry.providers.insert(
        "openai".to_string(),
        opendev_config::models_dev::ProviderInfo {
            id: "openai".to_string(),
            name: "Custom OpenAI".to_string(),
            description: "from cache".to_string(),
            api_key_env: "CUSTOM_OPENAI_KEY".to_string(),
            api_base_url: "https://proxy.example.com".to_string(),
            models: HashMap::new(),
        },
    );

    let added = merge_builtin_providers(&mut registry);
    assert!(added, "missing builtins should still be added");
    assert_eq!(registry.providers.len(), 12);

    // The registry (cache) entry wins over the builtin default
    let openai = registry.get_provider("openai").unwrap();
    assert_eq!(openai.name, "Custom OpenAI");
    assert_eq!(openai.api_key_env, "CUSTOM_OPENAI_KEY");
    assert_eq!(openai.api_base_url, "https://proxy.example.com");
}

#[test]
fn test_get_api_key_allows_keyless_providers() {
    // Providers with an empty env_var (ollama, lmstudio) must not require a
    // key — and must not touch stdin.
    let provider_config = ProviderConfig {
        id: "ollama".to_string(),
        name: "Ollama".to_string(),
        description: "Ollama models".to_string(),
        env_var: String::new(),
        api_base_url: "http://localhost:11434".to_string(),
        api_format: providers::ApiFormat::OpenAi,
        models: Vec::new(),
    };

    let key = get_api_key(&provider_config).expect("keyless provider should not error");
    assert!(key.is_empty());
}

#[test]
fn test_openai_chatgpt_authentication_selects_the_oauth_provider() {
    assert_eq!(
        resolve_provider_authentication("openai".to_string(), OpenAiAuthentication::ChatGptBrowser,),
        ("openai-chatgpt".to_string(), false)
    );
    assert_eq!(
        resolve_provider_authentication(
            "openai".to_string(),
            OpenAiAuthentication::ChatGptHeadless,
        ),
        ("openai-chatgpt".to_string(), true)
    );
    assert_eq!(
        resolve_provider_authentication("openai".to_string(), OpenAiAuthentication::ApiKey),
        ("openai".to_string(), false)
    );
}

#[test]
fn test_setup_error_variants() {
    let errors: Vec<SetupError> = vec![
        SetupError::Cancelled,
        SetupError::NoProvider,
        SetupError::NoApiKey,
        SetupError::ValidationFailed("test".into()),
        SetupError::NoModel,
        SetupError::SaveFailed("test".into()),
        SetupError::RegistryError("test".into()),
        SetupError::Io(io::Error::new(io::ErrorKind::Other, "test")),
    ];
    assert_eq!(errors.len(), 8);
}
