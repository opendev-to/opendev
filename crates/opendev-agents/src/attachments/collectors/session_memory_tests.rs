use super::*;

#[test]
fn test_summarize_messages_empty() {
    let messages = vec![];
    let summary = SessionMemoryCollector::summarize_messages(&messages);
    assert!(summary.is_empty());
}

#[test]
fn test_summarize_messages_basic() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "Hello"}),
        serde_json::json!({"role": "assistant", "content": "Hi there"}),
    ];
    let summary = SessionMemoryCollector::summarize_messages(&messages);
    assert!(summary.contains("[user]: Hello"));
    assert!(summary.contains("[assistant]: Hi there"));
}

#[test]
fn test_summarize_messages_truncates_long_content() {
    let long_content = "x".repeat(5000);
    let messages = vec![serde_json::json!({"role": "user", "content": long_content})];
    let summary = SessionMemoryCollector::summarize_messages(&messages);
    // Should be truncated to MAX_CHARS_PER_MESSAGE
    assert!(summary.len() < 5000);
}

#[test]
fn test_summarize_messages_limits_count() {
    let messages: Vec<serde_json::Value> = (0..50)
        .map(|i| serde_json::json!({"role": "user", "content": format!("msg {i}")}))
        .collect();
    let summary = SessionMemoryCollector::summarize_messages(&messages);
    // Should only include the last MAX_MESSAGES_FOR_EXTRACTION messages
    assert!(!summary.contains("msg 0"));
    assert!(summary.contains("msg 49"));
}

#[test]
fn test_should_fire_without_tokens() {
    let collector = SessionMemoryCollector::new();
    let ctx = crate::attachments::TurnContext {
        turn_number: 1,
        working_dir: std::path::Path::new("/tmp"),
        todo_manager: None,
        shared_state: None,
        last_user_query: None,
        cumulative_input_tokens: None,
        session_id: None,
        recent_messages: None,
    };
    assert!(!collector.should_fire(&ctx));
}

#[test]
fn test_should_fire_below_threshold() {
    let collector = SessionMemoryCollector::new();
    let ctx = crate::attachments::TurnContext {
        turn_number: 1,
        working_dir: std::path::Path::new("/tmp"),
        todo_manager: None,
        shared_state: None,
        last_user_query: None,
        cumulative_input_tokens: Some(30_000),
        session_id: None,
        recent_messages: None,
    };
    assert!(!collector.should_fire(&ctx));
}

#[test]
fn test_should_fire_above_threshold() {
    let collector = SessionMemoryCollector::new();
    let ctx = crate::attachments::TurnContext {
        turn_number: 1,
        working_dir: std::path::Path::new("/tmp"),
        todo_manager: None,
        shared_state: None,
        last_user_query: None,
        cumulative_input_tokens: Some(55_000),
        session_id: None,
        recent_messages: None,
    };
    assert!(collector.should_fire(&ctx));
}

#[test]
fn test_extract_first_description_with_frontmatter() {
    let content = "---\ntype: project\ndescription: \"Test desc\"\n---\n\nBody";
    assert_eq!(extract_first_description(content), "Test desc");
}

#[test]
fn test_extract_first_description_without_frontmatter() {
    let content = "# Heading\nFirst line of body\nSecond line";
    assert_eq!(extract_first_description(content), "First line of body");
}
