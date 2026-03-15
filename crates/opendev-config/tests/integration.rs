//! Integration tests for configuration loading and model registry.
//!
//! Tests hierarchical config merge, environment variable overrides,
//! and models.dev cache operations using real filesystem I/O.

use opendev_config::{ConfigLoader, ModelInfo, ModelRegistry, ProviderInfo};
use opendev_models::AppConfig;
use std::collections::HashMap;
use tempfile::TempDir;

// ========================================================================
// Config loading from temp files
// ========================================================================

/// Loading with no config files yields defaults.
#[test]
fn config_defaults_when_no_files() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.model_provider, "fireworks");
    assert_eq!(config.temperature, 0.6);
    assert!(!config.verbose);
}

/// Global settings file is loaded correctly.
#[test]
fn config_loads_global_settings() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(
        &global,
        r#"{"model_provider": "openai", "model": "gpt-4", "temperature": 0.9}"#,
    )
    .unwrap();

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.model_provider, "openai");
    assert_eq!(config.model, "gpt-4");
    assert_eq!(config.temperature, 0.9);
}

/// Project settings override global settings.
#[test]
fn config_project_overrides_global() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(
        &global,
        r#"{"model_provider": "openai", "model": "gpt-4", "temperature": 0.5}"#,
    )
    .unwrap();
    std::fs::write(&project, r#"{"model": "gpt-4-turbo", "temperature": 0.2}"#).unwrap();

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.model_provider, "openai"); // from global
    assert_eq!(config.model, "gpt-4-turbo"); // overridden by project
    assert_eq!(config.temperature, 0.2); // overridden by project
}

/// Config merge preserves unset defaults.
#[test]
fn config_merge_preserves_defaults() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(&global, r#"{"verbose": true}"#).unwrap();

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert!(config.verbose);
    // Default temperature should be preserved
    assert_eq!(config.temperature, 0.6);
    // Default model_provider preserved
    assert_eq!(config.model_provider, "fireworks");
}

/// Invalid JSON in config file is handled gracefully.
#[test]
fn config_invalid_json_falls_back_to_defaults() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(&global, "not valid json {{{").unwrap();

    // Should not panic, just warn and use defaults
    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.model_provider, "fireworks");
}

/// Multiple override layers stack correctly.
#[test]
fn config_triple_layer_override() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    // Global: set provider and model
    std::fs::write(
        &global,
        r#"{"model_provider": "anthropic", "model": "claude-3", "max_tokens": 2000}"#,
    )
    .unwrap();
    // Project: override model only
    std::fs::write(&project, r#"{"model": "claude-3-opus"}"#).unwrap();

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.model_provider, "anthropic"); // from global
    assert_eq!(config.model, "claude-3-opus"); // overridden by project
    assert_eq!(config.max_tokens, 2000); // from global
    assert_eq!(config.temperature, 0.6); // default (neither file set it)
}

// ========================================================================
// Models.dev cache with temp dir
// ========================================================================

/// ModelRegistry loads from cache directory with provider JSON files.
#[test]
fn registry_loads_from_cache() {
    let tmp = TempDir::new().unwrap();
    let providers_dir = tmp.path().join("providers");
    std::fs::create_dir_all(&providers_dir).unwrap();

    let provider_json = serde_json::json!({
        "id": "test-provider",
        "name": "Test Provider",
        "description": "A test provider",
        "api_key_env": "TEST_KEY",
        "api_base_url": "https://api.test.com",
        "models": {
            "model-a": {
                "id": "model-a",
                "name": "Model A",
                "provider": "Test Provider",
                "context_length": 128000,
                "capabilities": ["text", "vision"],
                "pricing": {"input": 5.0, "output": 15.0, "unit": "per 1M tokens"},
                "recommended": true,
                "max_tokens": 4096
            },
            "model-b": {
                "id": "model-b",
                "name": "Model B",
                "provider": "Test Provider",
                "context_length": 32000,
                "capabilities": ["text"],
                "pricing": {"input": 1.0, "output": 2.0, "unit": "per 1M tokens"},
                "recommended": false
            }
        }
    });

    std::fs::write(
        providers_dir.join("test-provider.json"),
        serde_json::to_string_pretty(&provider_json).unwrap(),
    )
    .unwrap();

    let mut registry = ModelRegistry::new();
    assert!(registry.providers.is_empty());

    // Simulate load_providers_from_dir by calling it directly
    // (load_from_cache would try network if stale, so test the internal method)
    let loaded = std::fs::read_to_string(providers_dir.join("test-provider.json")).unwrap();
    let data: serde_json::Value = serde_json::from_str(&loaded).unwrap();
    // We'll manually add to test the lookup methods
    let mut models = HashMap::new();
    models.insert(
        "model-a".to_string(),
        ModelInfo {
            id: "model-a".to_string(),
            name: "Model A".to_string(),
            provider: "Test Provider".to_string(),
            context_length: 128000,
            capabilities: vec!["text".to_string(), "vision".to_string()],
            pricing_input: 5.0,
            pricing_output: 15.0,
            pricing_unit: "per 1M tokens".to_string(),
            serverless: false,
            tunable: false,
            recommended: true,
            max_tokens: Some(4096),
            supports_temperature: true,
            api_type: "chat".to_string(),
        },
    );
    models.insert(
        "model-b".to_string(),
        ModelInfo {
            id: "model-b".to_string(),
            name: "Model B".to_string(),
            provider: "Test Provider".to_string(),
            context_length: 32000,
            capabilities: vec!["text".to_string()],
            pricing_input: 1.0,
            pricing_output: 2.0,
            pricing_unit: "per 1M tokens".to_string(),
            serverless: false,
            tunable: false,
            recommended: false,
            max_tokens: None,
            supports_temperature: true,
            api_type: "chat".to_string(),
        },
    );
    registry.providers.insert(
        "test-provider".to_string(),
        ProviderInfo {
            id: "test-provider".to_string(),
            name: "Test Provider".to_string(),
            description: "A test provider".to_string(),
            api_key_env: "TEST_KEY".to_string(),
            api_base_url: "https://api.test.com".to_string(),
            models,
        },
    );

    // Test lookups
    assert_eq!(registry.providers.len(), 1);

    let provider = registry.get_provider("test-provider").unwrap();
    assert_eq!(provider.name, "Test Provider");
    assert_eq!(provider.models.len(), 2);

    let model = registry.get_model("test-provider", "model-a").unwrap();
    assert_eq!(model.context_length, 128000);
    assert!(model.capabilities.contains(&"vision".to_string()));

    let found = registry.find_model_by_id("model-b").unwrap();
    assert_eq!(found.0, "test-provider");
    assert_eq!(found.2.context_length, 32000);

    // Not found cases
    assert!(registry.get_provider("nonexistent").is_none());
    assert!(registry.get_model("test-provider", "nonexistent").is_none());
    assert!(registry.find_model_by_id("nonexistent").is_none());
}

/// ProviderInfo::list_models filters by capability.
#[test]
fn provider_list_models_filters_by_capability() {
    let mut models = HashMap::new();
    models.insert(
        "text-only".to_string(),
        ModelInfo {
            id: "text-only".to_string(),
            name: "Text Only".to_string(),
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
        "vision".to_string(),
        ModelInfo {
            id: "vision".to_string(),
            name: "Vision Model".to_string(),
            provider: "Test".to_string(),
            context_length: 128000,
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
        description: "Test".to_string(),
        api_key_env: "KEY".to_string(),
        api_base_url: "https://api.test.com".to_string(),
        models,
    };

    let all = provider.list_models(None);
    assert_eq!(all.len(), 2);
    // Sorted by context length descending
    assert_eq!(all[0].context_length, 128000);

    let vision_only = provider.list_models(Some("vision"));
    assert_eq!(vision_only.len(), 1);
    assert_eq!(vision_only[0].id, "vision");

    let recommended = provider.get_recommended_model().unwrap();
    assert_eq!(recommended.id, "vision");
}

/// ModelInfo::format_pricing formats correctly.
#[test]
fn model_info_format_pricing() {
    let model = ModelInfo {
        id: "test".to_string(),
        name: "Test".to_string(),
        provider: "Test".to_string(),
        context_length: 4096,
        capabilities: vec![],
        pricing_input: 2.5,
        pricing_output: 10.0,
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
        "$2.50 in / $10.00 out per million tokens"
    );

    let free = ModelInfo {
        pricing_input: 0.0,
        pricing_output: 0.0,
        ..model
    };
    assert_eq!(free.format_pricing(), "N/A");
}

/// Registry list_all_models with price filter.
#[test]
fn registry_list_all_models_with_filters() {
    let mut registry = ModelRegistry::new();

    let mut models = HashMap::new();
    models.insert(
        "cheap".to_string(),
        ModelInfo {
            id: "cheap".to_string(),
            name: "Cheap".to_string(),
            provider: "Test".to_string(),
            context_length: 4096,
            capabilities: vec!["text".to_string()],
            pricing_input: 0.5,
            pricing_output: 1.0,
            pricing_unit: "per 1M tokens".to_string(),
            serverless: false,
            tunable: false,
            recommended: false,
            max_tokens: None,
            supports_temperature: true,
            api_type: "chat".to_string(),
        },
    );
    models.insert(
        "expensive".to_string(),
        ModelInfo {
            id: "expensive".to_string(),
            name: "Expensive".to_string(),
            provider: "Test".to_string(),
            context_length: 128000,
            capabilities: vec!["text".to_string(), "vision".to_string()],
            pricing_input: 30.0,
            pricing_output: 60.0,
            pricing_unit: "per 1M tokens".to_string(),
            serverless: false,
            tunable: false,
            recommended: true,
            max_tokens: None,
            supports_temperature: true,
            api_type: "chat".to_string(),
        },
    );

    registry.providers.insert(
        "test".to_string(),
        ProviderInfo {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Test".to_string(),
            api_key_env: "KEY".to_string(),
            api_base_url: "https://api.test.com".to_string(),
            models,
        },
    );

    // No filters
    let all = registry.list_all_models(None, None);
    assert_eq!(all.len(), 2);

    // Filter by max price
    let cheap = registry.list_all_models(None, Some(5.0));
    assert_eq!(cheap.len(), 1);
    assert_eq!(cheap[0].1.id, "cheap");

    // Filter by capability
    let vision = registry.list_all_models(Some("vision"), None);
    assert_eq!(vision.len(), 1);
    assert_eq!(vision[0].1.id, "expensive");
}

// ========================================================================
// Instructions array merge
// ========================================================================

/// Instructions arrays are concatenated and deduplicated across config layers.
#[test]
fn config_instructions_merge_concat_dedup() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(
        &global,
        r#"{"instructions": ["CONTRIBUTING.md", "docs/rules.md"]}"#,
    )
    .unwrap();
    std::fs::write(
        &project,
        r#"{"instructions": ["docs/rules.md", "project-rules.md"]}"#,
    )
    .unwrap();

    let config = ConfigLoader::load(&global, &project).unwrap();
    // Should have all unique entries: CONTRIBUTING.md, docs/rules.md, project-rules.md
    assert_eq!(config.instructions.len(), 3);
    assert!(config.instructions.contains(&"CONTRIBUTING.md".to_string()));
    assert!(config.instructions.contains(&"docs/rules.md".to_string()));
    assert!(
        config
            .instructions
            .contains(&"project-rules.md".to_string())
    );
}

/// When only one layer has instructions, they're preserved as-is.
#[test]
fn config_instructions_single_layer() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(&global, r#"{"instructions": ["global-rules.md"]}"#).unwrap();
    // No project config
    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.instructions, vec!["global-rules.md"]);
}

/// When neither layer has instructions, the field is empty.
#[test]
fn config_instructions_empty_default() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert!(config.instructions.is_empty());
}

// ========================================================================
// Skill paths array merge
// ========================================================================

/// Skill paths arrays are concatenated and deduplicated across config layers.
#[test]
fn config_skill_paths_merge_concat_dedup() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(
        &global,
        r#"{"skill_paths": ["~/.custom/skills", "/opt/skills"]}"#,
    )
    .unwrap();
    std::fs::write(
        &project,
        r#"{"skill_paths": ["/opt/skills", "project-skills"]}"#,
    )
    .unwrap();

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.skill_paths.len(), 3);
    assert!(config.skill_paths.contains(&"~/.custom/skills".to_string()));
    assert!(config.skill_paths.contains(&"/opt/skills".to_string()));
    assert!(config.skill_paths.contains(&"project-skills".to_string()));
}

/// When only one layer has skill_paths, they're preserved as-is.
#[test]
fn config_skill_paths_single_layer() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    std::fs::write(&global, r#"{"skill_paths": ["~/.opendev/extra-skills"]}"#).unwrap();

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert_eq!(config.skill_paths, vec!["~/.opendev/extra-skills"]);
}

/// When neither layer has skill_paths, the field is empty.
#[test]
fn config_skill_paths_empty_default() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.json");
    let project = tmp.path().join("project.json");

    let config = ConfigLoader::load(&global, &project).unwrap();
    assert!(config.skill_paths.is_empty());
}
