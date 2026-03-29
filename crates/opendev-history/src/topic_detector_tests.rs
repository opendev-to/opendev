use super::*;

#[test]
fn test_resolve_cheap_model_with_env() {
    // This test depends on environment, so we just verify the function
    // doesn't panic with any input
    let _ = resolve_cheap_model("openai");
    let _ = resolve_cheap_model("anthropic");
    let _ = resolve_cheap_model("unknown");
}

#[test]
fn test_get_api_key_unknown_provider() {
    assert!(get_api_key("nonexistent_provider").is_none());
}

#[test]
fn test_topic_result_deserialization() {
    let json = r#"{"isNewTopic": true, "title": "Auth Refactor"}"#;
    let result: TopicResult = serde_json::from_str(json).unwrap();
    assert!(result.is_new_topic);
    assert_eq!(result.title.as_deref(), Some("Auth Refactor"));
}

#[test]
fn test_topic_result_no_new_topic() {
    let json = r#"{"isNewTopic": false, "title": null}"#;
    let result: TopicResult = serde_json::from_str(json).unwrap();
    assert!(!result.is_new_topic);
    assert!(result.title.is_none());
}

#[test]
fn test_detector_disabled_without_key() {
    // With a nonsense provider, no key should be found
    let detector = TopicDetector::new("nonexistent_provider_xyz_12345");
    // May or may not be enabled depending on env, but should not panic
    let _ = detector.is_enabled();
}

#[test]
fn test_simple_message_clone() {
    let msg = SimpleMessage {
        role: "user".to_string(),
        content: "hello".to_string(),
    };
    let cloned = msg.clone();
    assert_eq!(cloned.role, "user");
    assert_eq!(cloned.content, "hello");
}

#[test]
fn test_api_endpoint() {
    assert_eq!(
        api_endpoint("openai"),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(
        api_endpoint("fireworks"),
        "https://api.fireworks.ai/inference/v1/chat/completions"
    );
}

#[test]
fn test_max_title_truncation() {
    let long_title = "a".repeat(100);
    let truncated = if long_title.len() > MAX_TITLE_LEN {
        &long_title[..MAX_TITLE_LEN]
    } else {
        &long_title
    };
    assert_eq!(truncated.len(), 50);
}

#[tokio::test]
async fn test_set_title_on_session_manager() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    mgr.create_session();

    let session_id = mgr.current_session().unwrap().id.clone();
    mgr.set_title(&session_id, "New Title").unwrap();

    let session = mgr.current_session().unwrap();
    assert_eq!(
        session.metadata.get("title").and_then(|v| v.as_str()),
        Some("New Title")
    );
    assert!(session.slug.is_some());
}

#[tokio::test]
async fn test_set_title_on_disk_session() {
    use opendev_models::Session;

    let tmp = tempfile::TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    // Create and save a session, but don't keep it as current
    let mut session = Session::new();
    session.id = "disk-title-test".to_string();
    mgr.save_session(&session).unwrap();

    // Create a different current session
    mgr.create_session();

    // Set title on the disk session
    mgr.set_title("disk-title-test", "Disk Title").unwrap();

    // Load and verify
    let loaded = mgr.load_session("disk-title-test").unwrap();
    assert_eq!(
        loaded.metadata.get("title").and_then(|v| v.as_str()),
        Some("Disk Title")
    );
    assert!(loaded.slug.is_some());
}

#[tokio::test]
async fn test_set_title_truncates() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    mgr.create_session();

    let session_id = mgr.current_session().unwrap().id.clone();
    let long_title = "a".repeat(100);
    mgr.set_title(&session_id, &long_title).unwrap();

    let session = mgr.current_session().unwrap();
    let title = session
        .metadata
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(title.len(), 50);
}

#[test]
fn test_detector_disabled_returns_none() {
    let detector = TopicDetector::new("nonexistent_provider_xyz_99999");
    // If no key found, detector should be disabled
    if !detector.is_enabled() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(detector.detect_title(&[], None));
        assert!(result.is_none());
    }
}

#[test]
fn test_empty_title_filtered() {
    // TopicResult with empty title should be filtered out by detect_title logic
    let json = r#"{"isNewTopic": true, "title": ""}"#;
    let result: TopicResult = serde_json::from_str(json).unwrap();
    assert!(result.is_new_topic);
    let title = result
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    assert!(title.is_none());
}

#[test]
fn test_title_trimming() {
    let json = r#"{"isNewTopic": true, "title": "  debug login flow  "}"#;
    let result: TopicResult = serde_json::from_str(json).unwrap();
    let title = result
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    assert_eq!(title.as_deref(), Some("debug login flow"));
}

#[test]
fn test_message_count_limiting() {
    let msgs: Vec<SimpleMessage> = (0..10)
        .map(|i| SimpleMessage {
            role: "user".to_string(),
            content: format!("message {i}"),
        })
        .collect();
    let recent: Vec<SimpleMessage> = if msgs.len() > MAX_RECENT_MESSAGES {
        msgs[msgs.len() - MAX_RECENT_MESSAGES..].to_vec()
    } else {
        msgs.to_vec()
    };
    assert_eq!(recent.len(), MAX_RECENT_MESSAGES);
    assert_eq!(recent[0].content, "message 6");
}

#[tokio::test]
async fn test_set_title_nonexistent_session() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let result = mgr.set_title("no-such-session", "Title");
    assert!(result.is_err());
}
