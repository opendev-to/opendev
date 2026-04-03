use super::*;

#[test]
fn test_registry_from_cache() {
    let tmp = tempfile::TempDir::new().unwrap();
    let providers_dir = tmp.path().join("providers");
    std::fs::create_dir_all(&providers_dir).unwrap();

    let provider_json = serde_json::json!({
        "id": "test-provider",
        "name": "Test Provider",
        "description": "A test provider",
        "api_key_env": "TEST_KEY",
        "api_base_url": "https://api.test.com",
        "models": {
            "model-1": {
                "id": "model-1",
                "name": "Model One",
                "provider": "Test Provider",
                "context_length": 4096,
                "capabilities": ["text"],
                "pricing": {"input": 1.0, "output": 2.0, "unit": "per 1M tokens"},
                "recommended": true
            }
        }
    });

    std::fs::write(
        providers_dir.join("test-provider.json"),
        serde_json::to_string_pretty(&provider_json).unwrap(),
    )
    .unwrap();

    let mut registry = ModelRegistry::new();
    assert!(registry.load_providers_from_dir(&providers_dir));
    assert_eq!(registry.providers.len(), 1);

    let provider = registry.get_provider("test-provider").unwrap();
    assert_eq!(provider.name, "Test Provider");
    assert_eq!(provider.models.len(), 1);

    let model = registry.get_model("test-provider", "model-1").unwrap();
    assert_eq!(model.context_length, 4096);

    let found = registry.find_model_by_id("model-1").unwrap();
    assert_eq!(found.0, "test-provider");
}

#[test]
fn test_provider_sort_order() {
    let mut ids = vec!["zebra", "openai", "alpha", "anthropic"];
    ids.sort_by(|a, b| provider_sort_key(a).cmp(&provider_sort_key(b)));
    assert_eq!(ids, vec!["openai", "anthropic", "alpha", "zebra"]);
}

#[test]
fn test_find_model_prefers_provider_with_api_key() {
    let mut registry = ModelRegistry::new();

    // Provider without API key set (use a unique env var name that won't exist)
    let no_key_env = "OPENDEV_TEST_NO_KEY_SET_12345";

    let mut models_a = HashMap::new();
    models_a.insert(
        "shared-model".to_string(),
        ModelInfo {
            id: "shared-model".to_string(),
            name: "Shared Model".to_string(),
            provider: "No Key Provider".to_string(),
            context_length: 4096,
            capabilities: vec!["text".to_string()],
            pricing_input: 1.0,
            pricing_output: 2.0,
            pricing_unit: "per 1M tokens".to_string(),
            recommended: false,
            max_tokens: None,
            supports_temperature: true,
            serverless: false,
            tunable: false,
            api_type: "chat".to_string(),
        },
    );
    registry.providers.insert(
        "no-key-provider".to_string(),
        ProviderInfo {
            id: "no-key-provider".to_string(),
            name: "No Key Provider".to_string(),
            description: String::new(),
            api_key_env: no_key_env.to_string(),
            api_base_url: String::new(),
            models: models_a,
        },
    );

    // Provider with empty api_key_env (no key required — always usable)
    let mut models_b = HashMap::new();
    models_b.insert(
        "shared-model".to_string(),
        ModelInfo {
            id: "shared-model".to_string(),
            name: "Shared Model".to_string(),
            provider: "Free Provider".to_string(),
            context_length: 4096,
            capabilities: vec!["text".to_string()],
            pricing_input: 1.0,
            pricing_output: 2.0,
            pricing_unit: "per 1M tokens".to_string(),
            recommended: false,
            max_tokens: None,
            supports_temperature: true,
            serverless: false,
            tunable: false,
            api_type: "chat".to_string(),
        },
    );
    registry.providers.insert(
        "free-provider".to_string(),
        ProviderInfo {
            id: "free-provider".to_string(),
            name: "Free Provider".to_string(),
            description: String::new(),
            api_key_env: String::new(),
            api_base_url: String::new(),
            models: models_b,
        },
    );

    // Should prefer the provider that doesn't require a missing API key
    let result = registry.find_model_by_id("shared-model").unwrap();
    assert_eq!(
        result.0, "free-provider",
        "Should prefer provider with available API key over one without"
    );
}

#[test]
fn test_builtin_provider_known() {
    let cases = vec![
        (
            "anthropic",
            "Anthropic",
            "ANTHROPIC_API_KEY",
            "https://api.anthropic.com",
        ),
        (
            "openai",
            "OpenAI",
            "OPENAI_API_KEY",
            "https://api.openai.com",
        ),
        ("ollama", "Ollama", "", "http://localhost:11434"),
        (
            "gemini",
            "Google Gemini",
            "GEMINI_API_KEY",
            "https://generativelanguage.googleapis.com",
        ),
        (
            "google",
            "Google Gemini",
            "GEMINI_API_KEY",
            "https://generativelanguage.googleapis.com",
        ),
        (
            "groq",
            "Groq",
            "GROQ_API_KEY",
            "https://api.groq.com/openai",
        ),
        (
            "deepseek",
            "DeepSeek",
            "DEEPSEEK_API_KEY",
            "https://api.deepseek.com",
        ),
        ("xai", "xAI", "XAI_API_KEY", "https://api.x.ai"),
        ("lmstudio", "LM Studio", "", "http://localhost:1234"),
    ];
    for (id, expected_name, expected_env, expected_url) in cases {
        let info = ModelRegistry::builtin_provider(id)
            .unwrap_or_else(|| panic!("builtin_provider should return Some for {id}"));
        assert_eq!(info.id, id);
        assert_eq!(info.name, expected_name);
        assert_eq!(info.api_key_env, expected_env);
        assert_eq!(info.api_base_url, expected_url);
        assert!(
            info.models.is_empty(),
            "builtin providers should have no models"
        );
    }
}

#[test]
fn test_builtin_provider_unknown_returns_none() {
    assert!(ModelRegistry::builtin_provider("nonexistent-provider").is_none());
    assert!(ModelRegistry::builtin_provider("").is_none());
}

#[test]
fn test_get_provider_or_builtin_prefers_registry() {
    let mut registry = ModelRegistry::new();

    // Insert a custom provider with the same id as a builtin
    let custom_url = "https://custom.anthropic.example.com";
    registry.providers.insert(
        "anthropic".to_string(),
        ProviderInfo {
            id: "anthropic".to_string(),
            name: "Custom Anthropic".to_string(),
            description: "Custom".to_string(),
            api_key_env: "CUSTOM_KEY".to_string(),
            api_base_url: custom_url.to_string(),
            models: HashMap::new(),
        },
    );

    let info = registry.get_provider_or_builtin("anthropic").unwrap();
    assert_eq!(
        info.name, "Custom Anthropic",
        "should prefer registry over builtin"
    );
    assert_eq!(info.api_base_url, custom_url);
    assert_eq!(info.api_key_env, "CUSTOM_KEY");
}

#[test]
fn test_get_provider_or_builtin_falls_back_when_empty() {
    let registry = ModelRegistry::new();
    assert!(registry.providers.is_empty());

    let info = registry.get_provider_or_builtin("openai").unwrap();
    assert_eq!(info.name, "OpenAI");
    assert_eq!(info.api_key_env, "OPENAI_API_KEY");
    assert_eq!(info.api_base_url, "https://api.openai.com");

    // Unknown provider still returns None
    assert!(registry.get_provider_or_builtin("nonexistent").is_none());
}
