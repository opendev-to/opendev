use super::*;
use serde_json::json;

fn make_msg(role: &str, content: &str) -> Value {
    json!({"role": role, "content": content})
}

#[test]
fn test_consolidate_learnings_error_fix_pattern() {
    let messages = vec![
        make_msg("user", "fix the build"),
        json!({
            "role": "tool",
            "content": "error: module 'foo' not found"
        }),
        json!({
            "role": "assistant",
            "content": "The issue was a missing import. Fixed by adding the module."
        }),
    ];
    let learnings = consolidate_learnings(&messages);
    assert!(
        learnings.iter().any(|l| l.contains("Error pattern fixed")),
        "Should detect error->fix pattern, got: {learnings:?}"
    );
}

#[test]
fn test_consolidate_learnings_config_discovery() {
    let messages = vec![
        make_msg("user", "how to connect to the database?"),
        json!({
            "role": "assistant",
            "content": "The database configuration is in config/database.yml with host and port settings."
        }),
    ];
    let learnings = consolidate_learnings(&messages);
    assert!(
        learnings
            .iter()
            .any(|l| l.contains("Configuration discovered")),
        "Should detect config discovery, got: {learnings:?}"
    );
}

#[test]
fn test_consolidate_learnings_file_patterns() {
    let messages = vec![json!({
        "role": "assistant",
        "content": "",
        "tool_calls": [
            {"function": {"name": "read_file", "arguments": "{\"path\": \"src/main.rs\"}"}},
            {"function": {"name": "edit_file", "arguments": "{\"path\": \"src/lib.rs\"}"}},
            {"function": {"name": "read_file", "arguments": "{\"path\": \"tests/test.py\"}"}}
        ]
    })];
    let learnings = consolidate_learnings(&messages);
    assert!(
        learnings.iter().any(|l| l.contains("File patterns used")),
        "Should detect file patterns, got: {learnings:?}"
    );
}

#[test]
fn test_consolidate_learnings_empty() {
    let messages: Vec<Value> = vec![];
    let learnings = consolidate_learnings(&messages);
    assert!(learnings.is_empty());
}

#[test]
fn test_consolidate_learnings_no_patterns() {
    let messages = vec![make_msg("user", "hello"), make_msg("assistant", "hi there")];
    let learnings = consolidate_learnings(&messages);
    assert!(learnings.is_empty());
}
