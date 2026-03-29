//! Model picker controller for selecting LLM models in the TUI.
//!
//! Provides a searchable, provider-grouped model selection popup.

use opendev_config::models_dev::{ModelInfo, ModelRegistry};

/// A model option displayed in the picker.
#[derive(Debug, Clone)]
pub struct ModelOption {
    /// Unique model identifier (e.g. "claude-sonnet-4").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Provider name (e.g. "anthropic").
    pub provider: String,
    /// Provider display name (e.g. "Anthropic").
    pub provider_display: String,
    /// Context window length in tokens.
    pub context_length: u64,
    /// Input pricing per million tokens.
    pub pricing_input: f64,
    /// Output pricing per million tokens.
    pub pricing_output: f64,
    /// Whether this is a recommended model.
    pub recommended: bool,
    /// Whether the provider's API key is available.
    pub has_api_key: bool,
}

/// Controller for navigating and selecting a model from a list.
pub struct ModelPickerController {
    /// All available models (unfiltered).
    all_models: Vec<ModelOption>,
    /// Filtered models matching the current search query.
    filtered_models: Vec<usize>,
    /// Current selected index into `filtered_models`.
    selected_index: usize,
    /// Whether the picker is currently active.
    active: bool,
    /// Current search/filter query.
    search_query: String,
    /// Scroll offset for the visible window.
    scroll_offset: usize,
    /// Maximum visible items in the popup.
    max_visible: usize,
}

impl ModelPickerController {
    /// Create a new picker with the given model options.
    pub fn new(models: Vec<ModelOption>) -> Self {
        let filtered: Vec<usize> = (0..models.len()).collect();
        Self {
            all_models: models,
            filtered_models: filtered,
            selected_index: 0,
            active: true,
            search_query: String::new(),
            scroll_offset: 0,
            max_visible: 15,
        }
    }

    /// Load models from the registry cache, grouped by provider.
    pub fn from_registry(cache_dir: &std::path::Path, current_model: &str) -> Self {
        let registry = ModelRegistry::load_from_cache(cache_dir);
        let mut models = Vec::new();

        // Get providers sorted by priority, with API-key-available providers first
        let mut providers = registry.list_providers();
        providers.sort_by_key(|p| {
            let has_key = p.api_key_env.is_empty() || std::env::var(&p.api_key_env).is_ok();
            !has_key // false < true, so has_key=true sorts first
        });
        for provider in &providers {
            let has_api_key =
                provider.api_key_env.is_empty() || std::env::var(&provider.api_key_env).is_ok();
            let mut provider_models: Vec<&ModelInfo> = provider.models.values().collect();
            // Sort: recommended first, then by context length descending
            provider_models.sort_by(|a, b| {
                b.recommended
                    .cmp(&a.recommended)
                    .then(b.context_length.cmp(&a.context_length))
            });
            for model in provider_models {
                models.push(ModelOption {
                    id: model.id.clone(),
                    name: model.name.clone(),
                    provider: provider.id.clone(),
                    provider_display: provider.name.clone(),
                    context_length: model.context_length,
                    pricing_input: model.pricing_input,
                    pricing_output: model.pricing_output,
                    recommended: model.recommended,
                    has_api_key,
                });
            }
        }

        let mut picker = Self::new(models);

        // Pre-select the current model
        if let Some(idx) = picker.all_models.iter().position(|m| m.id == current_model)
            && let Some(filtered_idx) = picker.filtered_models.iter().position(|&i| i == idx)
        {
            picker.selected_index = filtered_idx;
            // Ensure selected item is visible
            if picker.selected_index >= picker.max_visible {
                picker.scroll_offset = picker.selected_index.saturating_sub(picker.max_visible / 2);
            }
        }

        picker
    }

    /// Whether the picker is currently active.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The filtered model options to display.
    pub fn visible_models(&self) -> Vec<(usize, &ModelOption)> {
        self.filtered_models
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(self.max_visible)
            .map(|(i, &model_idx)| (i, &self.all_models[model_idx]))
            .collect()
    }

    /// Total number of filtered models.
    pub fn filtered_count(&self) -> usize {
        self.filtered_models.len()
    }

    /// The currently selected index in the filtered list.
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// The current search query.
    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    /// Move selection to the next item (wrapping).
    pub fn next(&mut self) {
        if self.filtered_models.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.filtered_models.len();
        self.ensure_visible();
    }

    /// Move selection to the previous item (wrapping).
    pub fn prev(&mut self) {
        if self.filtered_models.is_empty() {
            return;
        }
        self.selected_index =
            (self.selected_index + self.filtered_models.len() - 1) % self.filtered_models.len();
        self.ensure_visible();
    }

    /// Ensure the selected item is within the visible scroll window.
    fn ensure_visible(&mut self) {
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + self.max_visible {
            self.scroll_offset = self.selected_index + 1 - self.max_visible;
        }
    }

    /// Confirm the current selection and deactivate the picker.
    ///
    /// Returns `None` if the filtered list is empty.
    pub fn select(&mut self) -> Option<ModelOption> {
        if self.filtered_models.is_empty() {
            return None;
        }
        self.active = false;
        let model_idx = self.filtered_models[self.selected_index];
        Some(self.all_models[model_idx].clone())
    }

    /// Cancel the picker without selecting.
    pub fn cancel(&mut self) {
        self.active = false;
    }

    /// Add a character to the search query and re-filter.
    pub fn search_push(&mut self, c: char) {
        self.search_query.push(c);
        self.refilter();
    }

    /// Remove the last character from the search query and re-filter.
    pub fn search_pop(&mut self) {
        self.search_query.pop();
        self.refilter();
    }

    /// Re-filter models based on the current search query.
    fn refilter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_models = (0..self.all_models.len()).collect();
        } else {
            let query = self.search_query.to_lowercase();
            self.filtered_models = self
                .all_models
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    m.name.to_lowercase().contains(&query)
                        || m.id.to_lowercase().contains(&query)
                        || m.provider.to_lowercase().contains(&query)
                        || m.provider_display.to_lowercase().contains(&query)
                })
                .map(|(i, _)| i)
                .collect();
        }
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Format the context length for display (e.g. "128k", "1M").
    pub fn format_context(ctx: u64) -> String {
        if ctx >= 1_000_000 {
            format!("{}M", ctx / 1_000_000)
        } else if ctx >= 1000 {
            format!("{}k", ctx / 1000)
        } else {
            format!("{}", ctx)
        }
    }

    /// Format pricing for display.
    pub fn format_pricing(input: f64, output: f64) -> String {
        if input == 0.0 && output == 0.0 {
            "free".to_string()
        } else {
            format!("${:.2}/${:.2}", input, output)
        }
    }
}

#[cfg(test)]
#[path = "model_picker_tests.rs"]
mod tests;
