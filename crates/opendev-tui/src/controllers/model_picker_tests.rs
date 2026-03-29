use super::*;

fn sample_models() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "claude-sonnet-4".into(),
            name: "Claude Sonnet 4".into(),
            provider: "anthropic".into(),
            provider_display: "Anthropic".into(),
            context_length: 200_000,
            pricing_input: 3.0,
            pricing_output: 15.0,
            recommended: true,
            has_api_key: true,
        },
        ModelOption {
            id: "gpt-4o".into(),
            name: "GPT-4o".into(),
            provider: "openai".into(),
            provider_display: "OpenAI".into(),
            context_length: 128_000,
            pricing_input: 2.5,
            pricing_output: 10.0,
            recommended: true,
            has_api_key: true,
        },
        ModelOption {
            id: "gemini-2.5-pro".into(),
            name: "Gemini 2.5 Pro".into(),
            provider: "google".into(),
            provider_display: "Google".into(),
            context_length: 1_000_000,
            pricing_input: 1.25,
            pricing_output: 5.0,
            recommended: false,
            has_api_key: false,
        },
    ]
}

#[test]
fn test_new_picker() {
    let picker = ModelPickerController::new(sample_models());
    assert!(picker.active());
    assert_eq!(picker.selected_index(), 0);
    assert_eq!(picker.filtered_count(), 3);
}

#[test]
fn test_next_wraps() {
    let mut picker = ModelPickerController::new(sample_models());
    picker.next();
    assert_eq!(picker.selected_index(), 1);
    picker.next();
    assert_eq!(picker.selected_index(), 2);
    picker.next();
    assert_eq!(picker.selected_index(), 0); // wrap
}

#[test]
fn test_prev_wraps() {
    let mut picker = ModelPickerController::new(sample_models());
    picker.prev();
    assert_eq!(picker.selected_index(), 2); // wrap back
    picker.prev();
    assert_eq!(picker.selected_index(), 1);
}

#[test]
fn test_select() {
    let mut picker = ModelPickerController::new(sample_models());
    picker.next(); // select index 1
    let selected = picker.select().unwrap();
    assert_eq!(selected.id, "gpt-4o");
    assert!(!picker.active());
}

#[test]
fn test_select_empty() {
    let mut picker = ModelPickerController::new(vec![]);
    assert!(picker.select().is_none());
}

#[test]
fn test_cancel() {
    let mut picker = ModelPickerController::new(sample_models());
    picker.cancel();
    assert!(!picker.active());
}

#[test]
fn test_search_filters() {
    let mut picker = ModelPickerController::new(sample_models());
    picker.search_push('g');
    picker.search_push('p');
    picker.search_push('t');
    assert_eq!(picker.filtered_count(), 1);
    let visible = picker.visible_models();
    assert_eq!(visible[0].1.id, "gpt-4o");
}

#[test]
fn test_search_by_provider() {
    let mut picker = ModelPickerController::new(sample_models());
    picker.search_push('a');
    picker.search_push('n');
    picker.search_push('t');
    picker.search_push('h');
    assert_eq!(picker.filtered_count(), 1);
    let visible = picker.visible_models();
    assert_eq!(visible[0].1.provider, "anthropic");
}

#[test]
fn test_search_pop_restores() {
    let mut picker = ModelPickerController::new(sample_models());
    picker.search_push('x');
    picker.search_push('y');
    picker.search_push('z');
    assert_eq!(picker.filtered_count(), 0);
    picker.search_pop();
    picker.search_pop();
    picker.search_pop();
    assert_eq!(picker.filtered_count(), 3);
}

#[test]
fn test_next_on_empty_is_noop() {
    let mut picker = ModelPickerController::new(vec![]);
    picker.next(); // should not panic
    assert_eq!(picker.selected_index(), 0);
}

#[test]
fn test_format_context() {
    assert_eq!(ModelPickerController::format_context(1_000_000), "1M");
    assert_eq!(ModelPickerController::format_context(128_000), "128k");
    assert_eq!(ModelPickerController::format_context(500), "500");
}

#[test]
fn test_format_pricing() {
    assert_eq!(
        ModelPickerController::format_pricing(3.0, 15.0),
        "$3.00/$15.00"
    );
    assert_eq!(ModelPickerController::format_pricing(0.0, 0.0), "free");
}

#[test]
fn test_from_registry_loads_models() {
    // Load from real cache (if available) to verify picker works end-to-end
    let cache_dir = opendev_config::Paths::new(None).global_cache_dir();
    let picker = ModelPickerController::from_registry(&cache_dir, "gpt-4.1-mini");
    // In CI without cache, picker may have 0 models — that's OK.
    // On dev machines with OPENAI_API_KEY set and cache populated, expect models.
    if std::env::var("OPENAI_API_KEY").is_ok() {
        eprintln!(
            "Picker loaded {} models, active={}",
            picker.filtered_count(),
            picker.active()
        );
        // If we have the API key, we should have at least some OpenAI models
        assert!(
            picker.filtered_count() > 0,
            "Expected models to load from cache when OPENAI_API_KEY is set"
        );
        assert!(picker.active());
    }
}
