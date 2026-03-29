use super::*;

#[test]
fn test_model_info_display() {
    let model = ModelInfo {
        id: "gpt-4".to_string(),
        name: "GPT-4".to_string(),
        provider: "OpenAI".to_string(),
        context_length: 128_000,
        capabilities: vec!["text".to_string(), "vision".to_string()],
        pricing_input: 30.0,
        pricing_output: 60.0,
        pricing_unit: "per million tokens".to_string(),
        serverless: false,
        tunable: false,
        recommended: true,
        max_tokens: Some(4096),
        supports_temperature: true,
        api_type: "chat".to_string(),
    };
    let display = format!("{}", model);
    assert!(display.contains("GPT-4"));
    assert!(display.contains("128000"));
}

#[test]
fn test_model_info_pricing() {
    let model = ModelInfo {
        id: "test".to_string(),
        name: "Test".to_string(),
        provider: "Test".to_string(),
        context_length: 4096,
        capabilities: vec![],
        pricing_input: 1.5,
        pricing_output: 2.0,
        pricing_unit: "per million tokens".to_string(),
        serverless: false,
        tunable: false,
        recommended: false,
        max_tokens: None,
        supports_temperature: true,
        api_type: "chat".to_string(),
    };
    assert_eq!(
        model.format_pricing(),
        "$1.50 in / $2.00 out per million tokens"
    );

    let free = ModelInfo {
        pricing_input: 0.0,
        pricing_output: 0.0,
        ..model.clone()
    };
    assert_eq!(free.format_pricing(), "N/A");
}

#[test]
fn test_provider_list_models() {
    let mut models = HashMap::new();
    models.insert(
        "small".to_string(),
        ModelInfo {
            id: "small".to_string(),
            name: "Small".to_string(),
            provider: "Test".to_string(),
            context_length: 4096,
            capabilities: vec!["text".to_string()],
            pricing_input: 0.0,
            pricing_output: 0.0,
            pricing_unit: "per million tokens".to_string(),
            serverless: false,
            tunable: false,
            recommended: false,
            max_tokens: None,
            supports_temperature: true,
            api_type: "chat".to_string(),
        },
    );
    models.insert(
        "large".to_string(),
        ModelInfo {
            id: "large".to_string(),
            name: "Large".to_string(),
            provider: "Test".to_string(),
            context_length: 128_000,
            capabilities: vec!["text".to_string(), "vision".to_string()],
            pricing_input: 0.0,
            pricing_output: 0.0,
            pricing_unit: "per million tokens".to_string(),
            serverless: false,
            tunable: false,
            recommended: true,
            max_tokens: None,
            supports_temperature: true,
            api_type: "chat".to_string(),
        },
    );

    let provider = ProviderInfo {
        id: "test".to_string(),
        name: "Test".to_string(),
        description: "Test provider".to_string(),
        api_key_env: "TEST_API_KEY".to_string(),
        api_base_url: "https://api.test.com".to_string(),
        models,
    };

    let all = provider.list_models(None);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].context_length, 128_000); // sorted by context desc

    let vision = provider.list_models(Some("vision"));
    assert_eq!(vision.len(), 1);
    assert_eq!(vision[0].id, "large");

    assert_eq!(provider.get_recommended_model().unwrap().id, "large");
}
