use super::*;

#[test]
fn test_new_empty() {
    let mgr = AuthProfileManager::new("openai", vec![]);
    assert_eq!(mgr.profile_count(), 0);
    assert_eq!(mgr.available_count(), 0);
}

#[test]
fn test_new_filters_empty_keys() {
    let mgr = AuthProfileManager::new("openai", vec!["key1".into(), "".into(), "key2".into()]);
    assert_eq!(mgr.profile_count(), 2);
}

#[test]
fn test_get_active_key() {
    let mut mgr = AuthProfileManager::new("openai", vec!["sk-test".into()]);
    assert_eq!(mgr.get_active_key(), Some("sk-test"));
}

#[test]
fn test_get_active_key_empty() {
    let mut mgr = AuthProfileManager::new("openai", vec![]);
    assert_eq!(mgr.get_active_key(), None);
}

#[test]
fn test_mark_success() {
    let mut mgr = AuthProfileManager::new("openai", vec!["sk-test".into()]);
    mgr.mark_success();
    assert_eq!(mgr.profiles[0].request_count, 1);
    assert_eq!(mgr.profiles[0].failure_count, 0);
}

#[test]
fn test_mark_failure_and_rotate() {
    let mut mgr = AuthProfileManager::new("openai", vec!["key-a".into(), "key-b".into()]);
    assert_eq!(mgr.get_active_key(), Some("key-a"));

    // Fail key-a, should rotate to key-b
    mgr.mark_failure(429);
    assert_eq!(mgr.get_active_key(), Some("key-b"));
}

#[test]
fn test_all_keys_in_cooldown() {
    let mut mgr = AuthProfileManager::new("openai", vec!["key-a".into(), "key-b".into()]);
    mgr.mark_failure(429);
    mgr.current_index = 1;
    mgr.mark_failure(429);

    assert_eq!(mgr.get_active_key(), None);
    assert_eq!(mgr.available_count(), 0);
}

#[test]
fn test_success_resets_cooldown() {
    let mut mgr = AuthProfileManager::new("openai", vec!["sk-test".into()]);
    mgr.mark_failure(429);
    assert!(!mgr.profiles[0].is_available());

    mgr.mark_success();
    assert!(mgr.profiles[0].is_available());
    assert_eq!(mgr.available_count(), 1);
}

#[test]
fn test_from_config_api_keys() {
    let mut config = HashMap::new();
    config.insert("api_keys".into(), serde_json::json!(["key1", "key2"]));
    let mgr = AuthProfileManager::from_config("openai", &config);
    assert_eq!(mgr.profile_count(), 2);
}

#[test]
fn test_from_config_single_key() {
    let mut config = HashMap::new();
    config.insert("api_key".into(), serde_json::json!("single-key"));
    let mgr = AuthProfileManager::from_config("openai", &config);
    assert_eq!(mgr.profile_count(), 1);
    assert_eq!(mgr.provider(), "openai");
}

#[test]
fn test_from_config_empty() {
    let config = HashMap::new();
    let mgr = AuthProfileManager::from_config("openai", &config);
    assert_eq!(mgr.profile_count(), 0);
}

#[test]
fn test_cooldown_seconds() {
    assert_eq!(cooldown_seconds(429), 30.0);
    assert_eq!(cooldown_seconds(401), 300.0);
    assert_eq!(cooldown_seconds(403), 600.0);
    assert_eq!(cooldown_seconds(999), 60.0); // default
}

#[test]
fn test_debug_format() {
    let mgr = AuthProfileManager::new("anthropic", vec!["key".into()]);
    let debug = format!("{:?}", mgr);
    assert!(debug.contains("anthropic"));
    assert!(debug.contains("profile_count"));
}
