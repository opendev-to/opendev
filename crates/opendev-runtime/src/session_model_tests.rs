use super::*;
use std::cell::RefCell;

#[test]
fn test_new_manager_inactive() {
    let mgr = SessionModelManager::new();
    assert!(!mgr.is_active());
    assert!(mgr.get_overlay().is_none());
}

#[test]
fn test_apply_overlay() {
    let mut mgr = SessionModelManager::new();
    let config: RefCell<HashMap<String, String>> = RefCell::new(HashMap::from([
        ("model".into(), "gpt-4".into()),
        ("model_provider".into(), "openai".into()),
    ]));

    let overlay: SessionOverlay = HashMap::from([
        ("model".into(), "claude-3-opus".into()),
        ("model_provider".into(), "anthropic".into()),
    ]);

    mgr.apply(
        &overlay,
        |key| config.borrow().get(key).cloned(),
        |key, value| {
            config
                .borrow_mut()
                .insert(key.to_string(), value.to_string());
        },
    );

    assert!(mgr.is_active());
    assert_eq!(config.borrow().get("model").unwrap(), "claude-3-opus");
    assert_eq!(config.borrow().get("model_provider").unwrap(), "anthropic");
}

#[test]
fn test_restore_overlay() {
    let mut mgr = SessionModelManager::new();
    let config: RefCell<HashMap<String, String>> =
        RefCell::new(HashMap::from([("model".into(), "gpt-4".into())]));

    let overlay: SessionOverlay = HashMap::from([("model".into(), "claude-3-opus".into())]);

    mgr.apply(
        &overlay,
        |key| config.borrow().get(key).cloned(),
        |key, value| {
            config
                .borrow_mut()
                .insert(key.to_string(), value.to_string());
        },
    );

    assert_eq!(config.borrow().get("model").unwrap(), "claude-3-opus");

    mgr.restore(|key, value| {
        config
            .borrow_mut()
            .insert(key.to_string(), value.to_string());
    });

    assert!(!mgr.is_active());
    assert_eq!(config.borrow().get("model").unwrap(), "gpt-4");
}

#[test]
fn test_apply_empty_overlay_is_noop() {
    let mut mgr = SessionModelManager::new();
    mgr.apply(&HashMap::new(), |_| None, |_, _| {});
    assert!(!mgr.is_active());
}

#[test]
fn test_apply_ignores_invalid_fields() {
    let mut mgr = SessionModelManager::new();
    let config: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());

    let overlay: SessionOverlay = HashMap::from([("invalid_field".into(), "value".into())]);

    mgr.apply(
        &overlay,
        |key| config.borrow().get(key).cloned(),
        |key, value| {
            config
                .borrow_mut()
                .insert(key.to_string(), value.to_string());
        },
    );

    // overlay is set but no config was changed
    assert!(mgr.is_active());
    assert!(config.borrow().is_empty());
}

#[test]
fn test_get_set_clear_session_model() {
    let mut metadata = serde_json::json!({});

    // Set
    let overlay: SessionOverlay = HashMap::from([("model".into(), "claude-3-opus".into())]);
    set_session_model(&mut metadata, &overlay);

    // Get
    let retrieved = get_session_model(&metadata);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().get("model").unwrap(), "claude-3-opus");

    // Clear
    clear_session_model(&mut metadata);
    assert!(get_session_model(&metadata).is_none());
}

#[test]
fn test_validate_session_model() {
    let overlay: SessionOverlay = HashMap::from([
        ("model".into(), "gpt-4".into()),
        ("model_provider".into(), "openai".into()),
        ("garbage_field".into(), "value".into()),
    ]);

    let (valid, warnings) = validate_session_model(&overlay);
    assert_eq!(valid.len(), 2);
    assert!(valid.contains_key("model"));
    assert!(valid.contains_key("model_provider"));
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("garbage_field"));
}

#[test]
fn test_validate_empty() {
    let (valid, warnings) = validate_session_model(&HashMap::new());
    assert!(valid.is_empty());
    assert!(warnings.is_empty());
}

#[test]
fn test_session_model_fields_contains_expected() {
    assert!(SESSION_MODEL_FIELDS.contains(&"model"));
    assert!(SESSION_MODEL_FIELDS.contains(&"model_thinking"));
    assert!(SESSION_MODEL_FIELDS.contains(&"model_vlm_provider"));
}
