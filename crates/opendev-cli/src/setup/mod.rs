//! Interactive setup wizard for first-run configuration.
//!
//! Clean 3-section wizard: Provider & Auth → Model Selection → Optional Slots.

mod interactive_menu;
pub mod providers;
mod rail_ui;

use std::collections::HashMap;
use std::io;

use opendev_config::models_dev::ModelRegistry;
use opendev_config::{ConfigLoader, Paths};
use opendev_models::AppConfig;
use thiserror::Error;
use tracing::info;

use interactive_menu::InteractiveMenu;
use providers::{ProviderConfig, ProviderSetup};
use rail_ui::*;

// ── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum SetupError {
    #[error("setup cancelled by user")]
    Cancelled,
    #[error("no provider selected")]
    NoProvider,
    #[error("no API key provided")]
    NoApiKey,
    #[error("API key validation failed: {0}")]
    ValidationFailed(String),
    #[error("no model selected")]
    NoModel,
    #[error("failed to save configuration: {0}")]
    SaveFailed(String),
    #[error("model registry unavailable: {0}")]
    RegistryError(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Check whether a global settings file already exists.
pub fn config_exists() -> bool {
    let paths = Paths::default();
    paths.global_settings().exists()
}

/// Run the interactive setup wizard.
///
/// Returns the final [`AppConfig`] on success.
pub async fn run_setup_wizard() -> Result<AppConfig, SetupError> {
    let paths = Paths::default();
    let cache_dir = paths.global_cache_dir();
    let registry = ModelRegistry::load_from_cache(&cache_dir);
    if registry.providers.is_empty() {
        return Err(SetupError::RegistryError(
            "No provider data available. Check network connectivity and retry.".into(),
        ));
    }

    // ─── Welcome ────────────────────────────────────────────────────────
    rail_intro();

    // ─── Section 1: Provider & Authentication ───────────────────────────
    rail_section("Provider & Authentication");

    let provider_id = select_provider(&registry)?;
    let provider_config = ProviderSetup::get_provider_config(&registry, &provider_id)
        .ok_or(SetupError::NoProvider)?;

    let api_key = get_api_key(&provider_config)?;

    if rail_confirm("Validate API key?", true)? {
        let spinner = rail_spinner_start("Validating API key...");
        let result = ProviderSetup::validate_api_key(&provider_config, &api_key).await;
        spinner.stop();
        match result {
            Ok(()) => rail_success("API key is valid"),
            Err(e) => {
                rail_error(&format!("Validation failed: {e}"));
                rail_dim("Continuing anyway. You can fix this later in settings.");
            }
        }
    }

    // ─── Section 2: Model Selection ─────────────────────────────────────
    rail_section("Model Selection");

    let model_id = select_model(&provider_id, &provider_config, &registry)?;

    let normal_model_info = registry.find_model_by_id(&model_id);
    let normal_model_name = normal_model_info
        .map(|(_, _, m)| m.name.as_str())
        .unwrap_or("your model");

    // ─── Section 3: Specialized Models (optional) ───────────────────────
    rail_section("Specialized Models");
    rail_dim("These use your main model by default. Override only if needed.");

    let mut collected_keys: HashMap<String, String> = HashMap::new();
    collected_keys.insert(provider_id.clone(), api_key.clone());

    let (vlm_provider, vlm_model) = configure_slot(
        &registry,
        "Vision",
        "image & screenshot analysis",
        normal_model_name,
        &provider_id,
        &model_id,
        &mut collected_keys,
    )?;

    let (compact_provider, compact_model) = configure_slot(
        &registry,
        "Compact",
        "context summarization",
        normal_model_name,
        &provider_id,
        &model_id,
        &mut collected_keys,
    )?;

    // ─── Summary & Save ─────────────────────────────────────────────────
    let config = AppConfig {
        model_provider: provider_id.clone(),
        model: model_id.clone(),
        api_key: Some(api_key),
        auto_save_interval: 5,
        model_vlm: Some(vlm_model.clone()),
        model_vlm_provider: Some(vlm_provider.clone()),
        model_compact: Some(compact_model.clone()),
        model_compact_provider: Some(compact_provider.clone()),
        ..AppConfig::default()
    };

    show_summary(
        &registry,
        &provider_id,
        &model_id,
        &vlm_provider,
        &vlm_model,
        &compact_provider,
        &compact_model,
        &collected_keys,
    );

    if !rail_confirm("Save this configuration?", true)? {
        rail_dim("Setup cancelled.");
        return Err(SetupError::Cancelled);
    }

    let settings_path = paths.global_settings();
    ConfigLoader::save(&config, &settings_path)
        .map_err(|e| SetupError::SaveFailed(e.to_string()))?;

    info!(path = %settings_path.display(), "Configuration saved");
    rail_success(&format!("Saved to {}", settings_path.display()));
    rail_outro();

    Ok(config)
}

// ── Slot configuration ─────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn configure_slot(
    registry: &ModelRegistry,
    slot_name: &str,
    slot_desc: &str,
    normal_model_name: &str,
    normal_provider_id: &str,
    normal_model_id: &str,
    collected_keys: &mut HashMap<String, String>,
) -> Result<(String, String), SetupError> {
    let menu_items = vec![
        (
            "use_normal".to_string(),
            format!("Use {normal_model_name}"),
            "same as main model".to_string(),
        ),
        (
            "choose_manually".to_string(),
            "Choose different model".to_string(),
            format!("pick a model for {slot_desc}"),
        ),
    ];

    rail_label(&format!("{slot_name} model"), slot_desc);
    let mut menu = InteractiveMenu::new(menu_items, &format!("{slot_name} Model"), 2);
    let choice = menu.show()?;

    if choice.as_deref() != Some("choose_manually") {
        rail_dim(&format!("  Using {normal_model_name}"));
        return Ok((normal_provider_id.to_string(), normal_model_id.to_string()));
    }

    // Manual provider/model sub-flow
    let slot_provider_id = match select_provider(registry) {
        Ok(id) => id,
        Err(_) => return Ok((normal_provider_id.to_string(), normal_model_id.to_string())),
    };

    let slot_provider_config = match ProviderSetup::get_provider_config(registry, &slot_provider_id)
    {
        Some(c) => c,
        None => {
            rail_error(&format!("Provider '{slot_provider_id}' not found"));
            return Ok((normal_provider_id.to_string(), normal_model_id.to_string()));
        }
    };

    if !collected_keys.contains_key(&slot_provider_id) {
        let slot_api_key = match get_api_key(&slot_provider_config) {
            Ok(key) => key,
            Err(_) => {
                return Ok((normal_provider_id.to_string(), normal_model_id.to_string()));
            }
        };
        if rail_confirm("Validate API key?", true)? {
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    let spinner = rail_spinner_start("Validating API key...");
                    let result = handle.block_on(ProviderSetup::validate_api_key(
                        &slot_provider_config,
                        &slot_api_key,
                    ));
                    spinner.stop();
                    match result {
                        Ok(()) => rail_success("API key is valid"),
                        Err(e) => {
                            rail_error(&format!("Validation failed: {e}"));
                            rail_dim("Continuing anyway.");
                        }
                    }
                }
                Err(_) => {
                    rail_dim("Cannot validate in sync context, skipping.");
                }
            }
        }
        collected_keys.insert(slot_provider_id.clone(), slot_api_key);
    } else {
        rail_dim(&format!(
            "  Using existing API key for {}",
            slot_provider_config.name
        ));
    }

    let slot_model_id = match select_model(&slot_provider_id, &slot_provider_config, registry) {
        Ok(id) => id,
        Err(_) => return Ok((normal_provider_id.to_string(), normal_model_id.to_string())),
    };

    Ok((slot_provider_id, slot_model_id))
}

// ── Summary ─────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn show_summary(
    registry: &ModelRegistry,
    provider_id: &str,
    model_id: &str,
    vlm_provider: &str,
    vlm_model: &str,
    compact_provider: &str,
    compact_model: &str,
    collected_keys: &HashMap<String, String>,
) {
    let model_display = |pid: &str, mid: &str| -> String {
        let provider_name = registry
            .get_provider(pid)
            .map(|p| p.name.as_str())
            .unwrap_or(pid);
        let model_name = registry
            .find_model_by_id(mid)
            .map(|(_, _, m)| m.name.as_str())
            .unwrap_or(mid);
        format!("{provider_name} / {model_name}")
    };

    let normal_display = model_display(provider_id, model_id);

    let vlm_display = if vlm_model == model_id && vlm_provider == provider_id {
        "(same)".to_string()
    } else {
        model_display(vlm_provider, vlm_model)
    };

    let compact_display = if compact_model == model_id && compact_provider == provider_id {
        "(same)".to_string()
    } else {
        model_display(compact_provider, compact_model)
    };

    let rows: Vec<(&str, &str)> = vec![
        ("Main", &normal_display),
        ("Vision", &vlm_display),
        ("Compact", &compact_display),
    ];

    let mut key_lines: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for pid in collected_keys.keys() {
        if seen.contains(pid) {
            continue;
        }
        seen.push(pid.clone());
        let env_var = registry
            .get_provider(pid)
            .map(|p| p.api_key_env.as_str())
            .unwrap_or("")
            .to_string();
        let env_set = std::env::var(&env_var).is_ok();
        let status = if env_set { "set" } else { "configured" };
        key_lines.push(format!("${env_var} ({status})"));
    }

    rail_summary_box(&rows, &key_lines);
}

// ── Step helpers ────────────────────────────────────────────────────────────

fn select_provider(registry: &ModelRegistry) -> Result<String, SetupError> {
    let all_choices = ProviderSetup::provider_choices(registry);

    let priority_ids: &[&str] = &[
        "openai",
        "anthropic",
        "fireworks",
        "fireworks-ai",
        "google",
        "deepseek",
        "groq",
        "mistral",
        "cohere",
        "perplexity",
        "togetherai",
        "together",
    ];

    let mut popular: Vec<(String, String, String)> = all_choices
        .iter()
        .filter(|(id, _, _)| priority_ids.contains(&id.as_str()))
        .cloned()
        .collect();
    popular.sort_by_key(|(id, _, _)| {
        priority_ids
            .iter()
            .position(|&p| p == id)
            .unwrap_or(usize::MAX)
    });

    let has_more = all_choices.len() > popular.len();
    if has_more {
        popular.push((
            "__show_all__".to_string(),
            "Show all providers".to_string(),
            format!("{} providers available", all_choices.len()),
        ));
    }

    rail_label("Provider", "choose your AI provider");
    let mut menu = InteractiveMenu::new(popular, "Select AI Provider", 9);
    let mut provider_id = menu.show()?.ok_or(SetupError::Cancelled)?;

    if provider_id == "__show_all__" {
        let mut menu = InteractiveMenu::new(all_choices.clone(), "Select AI Provider", 9);
        provider_id = menu.show()?.ok_or(SetupError::Cancelled)?;
    }

    let provider_name = all_choices
        .iter()
        .find(|(id, _, _)| id == &provider_id)
        .map(|(_, name, _)| name.as_str())
        .unwrap_or(&provider_id);
    rail_answer(provider_name);
    Ok(provider_id)
}

fn get_api_key(provider_config: &ProviderConfig) -> Result<String, SetupError> {
    let env_var = &provider_config.env_var;
    let env_key = std::env::var(env_var).ok().filter(|k| !k.is_empty());

    if let Some(ref ek) = env_key
        && rail_confirm(&format!("Found ${env_var} in environment. Use it?"), true)?
    {
        rail_success("Using API key from environment");
        return Ok(ek.clone());
    }

    let api_key = rail_prompt(
        &format!("Enter your {} API key:", provider_config.name),
        true,
    )?;

    if api_key.is_empty() {
        if let Some(ek) = env_key {
            rail_success(&format!("Using ${env_var}"));
            return Ok(ek);
        }
        rail_error("No API key provided");
        return Err(SetupError::NoApiKey);
    }

    rail_success("API key received");
    Ok(api_key)
}

fn select_model(
    provider_id: &str,
    provider_config: &ProviderConfig,
    registry: &ModelRegistry,
) -> Result<String, SetupError> {
    let models = ProviderSetup::get_provider_models(registry, provider_id);

    let mut model_choices: Vec<(String, String, String)> = models;
    model_choices.push((
        "__custom__".to_string(),
        "Custom Model".to_string(),
        "Enter a custom model ID".to_string(),
    ));

    rail_label("Model", &format!("select a {} model", provider_config.name));
    let mut menu = InteractiveMenu::new(model_choices.clone(), "Select Model", 9);
    let model_id = menu.show()?.ok_or(SetupError::Cancelled)?;

    if model_id == "__custom__" {
        let custom_id = rail_prompt("Enter custom model ID:", false)?;
        if custom_id.is_empty() {
            rail_dim("No model ID provided");
            return Err(SetupError::NoModel);
        }
        rail_answer(&custom_id);
        return Ok(custom_id);
    }

    let model_name = model_choices
        .iter()
        .find(|(id, _, _)| id == &model_id)
        .map(|(_, name, _)| name.as_str())
        .unwrap_or(&model_id);
    rail_answer(model_name);
    Ok(model_id)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
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
}
