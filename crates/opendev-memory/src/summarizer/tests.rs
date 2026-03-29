use super::*;
use serde_json::json;

fn make_msg(role: &str, content: &str) -> Value {
    json!({"role": role, "content": content})
}

fn make_assistant_tool_call(tool_name: &str) -> Value {
    json!({
        "role": "assistant",
        "content": "",
        "tool_calls": [{"function": {"name": tool_name}}]
    })
}

#[test]
fn test_new_defaults() {
    let s = ConversationSummarizer::new();
    assert!(s.cache.is_none());
    assert_eq!(s.regenerate_threshold, 5);
    assert_eq!(s.max_summary_length, 500);
    assert_eq!(s.exclude_last_n, 6);
}

#[test]
fn test_builder_pattern() {
    let s = ConversationSummarizer::new()
        .with_threshold(10)
        .with_max_length(200)
        .with_exclude_last(3);
    assert_eq!(s.regenerate_threshold, 10);
    assert_eq!(s.max_summary_length, 200);
    assert_eq!(s.exclude_last_n, 3);
}

#[test]
fn test_needs_regeneration_no_cache() {
    let s = ConversationSummarizer::new();
    assert!(s.needs_regeneration(0));
    assert!(s.needs_regeneration(100));
}

#[test]
fn test_needs_regeneration_with_cache() {
    let mut s = ConversationSummarizer::new().with_threshold(5);
    s.cache = Some(ConversationSummary {
        summary: "test".into(),
        message_count: 10,
        last_summarized_index: 4,
    });
    assert!(!s.needs_regeneration(10));
    assert!(!s.needs_regeneration(14));
    assert!(s.needs_regeneration(15));
    assert!(s.needs_regeneration(20));
}

#[test]
fn test_get_cached_summary() {
    let mut s = ConversationSummarizer::new();
    assert_eq!(s.get_cached_summary(), None);

    s.cache = Some(ConversationSummary {
        summary: "hello world".into(),
        message_count: 5,
        last_summarized_index: 2,
    });
    assert_eq!(s.get_cached_summary(), Some("hello world"));
}

#[test]
fn test_generate_summary_too_few_messages() {
    let mut s = ConversationSummarizer::new().with_exclude_last(6);
    let messages: Vec<Value> = (0..5)
        .map(|i| make_msg("user", &format!("msg {i}")))
        .collect();
    let result = s.generate_summary(&messages, |_| Some("summary".into()));
    assert_eq!(result, ""); // Not enough messages after excluding last 6
}

#[test]
fn test_generate_summary_basic() {
    let mut s = ConversationSummarizer::new()
        .with_exclude_last(2)
        .with_max_length(100);

    let messages = vec![
        make_msg("user", "hello"),
        make_msg("assistant", "hi there"),
        make_msg("user", "how are you?"),
        make_msg("assistant", "I'm good"),
        make_msg("user", "recent 1"),
        make_msg("assistant", "recent 2"),
    ];

    let result = s.generate_summary(&messages, |_| Some("A greeting exchange.".into()));
    assert_eq!(result, "A greeting exchange.");
    assert!(s.cache.is_some());

    let cached = s.cache.as_ref().unwrap();
    assert_eq!(cached.message_count, 6);
    assert_eq!(cached.last_summarized_index, 4); // 6 - 2
}

#[test]
fn test_generate_summary_truncates() {
    let mut s = ConversationSummarizer::new()
        .with_exclude_last(1)
        .with_max_length(10);

    let messages = vec![
        make_msg("user", "hello"),
        make_msg("assistant", "world"),
        make_msg("user", "last"),
    ];

    let result = s.generate_summary(&messages, |_| {
        Some("This is a very long summary text".into())
    });
    assert_eq!(result.len(), 10);
    assert_eq!(result, "This is a ");
}

#[test]
fn test_generate_summary_filters_system() {
    let mut s = ConversationSummarizer::new()
        .with_exclude_last(1)
        .with_max_length(500);

    let messages = vec![
        make_msg("system", "You are a bot"),
        make_msg("user", "hello"),
        make_msg("assistant", "hi"),
        make_msg("user", "last"),
    ];

    // After filtering system, we have 3 messages. Exclude last 1 -> end_index=2.
    // new_messages = filtered[0..2] = ["hello", "hi"]
    let result = s.generate_summary(&messages, |call_msgs| {
        // Verify system messages are filtered from the conversation
        let prompt = call_msgs[1]["content"].as_str().unwrap();
        assert!(prompt.contains("User: hello"));
        assert!(prompt.contains("Assistant: hi"));
        assert!(!prompt.contains("You are a bot"));
        Some("Summary without system msg.".into())
    });
    assert_eq!(result, "Summary without system msg.");
}

#[test]
fn test_format_conversation() {
    let messages = vec![
        make_msg("user", "hello"),
        make_msg("assistant", "hi there"),
        make_assistant_tool_call("bash"),
        make_msg("tool", "some output"),
    ];

    let formatted = ConversationSummarizer::format_conversation(&messages);
    assert!(formatted.contains("User: hello"));
    assert!(formatted.contains("Assistant: hi there"));
    assert!(formatted.contains("Assistant: [Called tools: bash]"));
    assert!(formatted.contains("Tool: [result received]"));
}

#[test]
fn test_clear_cache_and_json_roundtrip() {
    let mut s = ConversationSummarizer::new();

    // No cache -> to_json returns None
    assert!(s.to_json().is_none());

    // Set cache
    s.cache = Some(ConversationSummary {
        summary: "test summary".into(),
        message_count: 10,
        last_summarized_index: 4,
    });

    // to_json
    let json_val = s.to_json().unwrap();
    assert_eq!(json_val["summary"], "test summary");
    assert_eq!(json_val["message_count"], 10);
    assert_eq!(json_val["last_summarized_index"], 4);

    // clear
    s.clear_cache();
    assert!(s.cache.is_none());
    assert_eq!(s.get_cached_summary(), None);

    // from_json
    s.from_json(Some(&json_val));
    assert_eq!(s.get_cached_summary(), Some("test summary"));

    let cached = s.cache.as_ref().unwrap();
    assert_eq!(cached.message_count, 10);
    assert_eq!(cached.last_summarized_index, 4);

    // from_json(None) clears
    s.from_json(None);
    assert!(s.cache.is_none());
}
