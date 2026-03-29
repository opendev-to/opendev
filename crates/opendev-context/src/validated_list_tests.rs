use super::*;

fn tc(id: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "function": { "name": name, "arguments": "{}" }
    })
}

#[test]
fn test_basic_user_assistant_flow() {
    let mut list = ValidatedMessageList::new(None, false);
    list.add_user("hello");
    list.add_assistant(Some("hi there"), None);
    assert_eq!(list.len(), 2);
    assert!(!list.has_pending_tools());
}

#[test]
fn test_tool_call_flow() {
    let mut list = ValidatedMessageList::new(None, false);
    list.add_user("run ls");
    list.add_assistant(None, Some(vec![tc("tc-1", "bash")]));
    assert!(list.has_pending_tools());
    assert!(list.pending_tool_ids().contains("tc-1"));

    list.add_tool_result("tc-1", "file1.txt\nfile2.txt")
        .unwrap();
    assert!(!list.has_pending_tools());
    assert_eq!(list.len(), 3);
}

#[test]
fn test_auto_complete_pending_on_new_user() {
    let mut list = ValidatedMessageList::new(None, false);
    list.add_assistant(None, Some(vec![tc("tc-1", "bash")]));
    assert!(list.has_pending_tools());

    // Adding a user message auto-completes the pending tool result
    list.add_user("next question");
    assert!(!list.has_pending_tools());
    // Should have: assistant + synthetic tool result + user = 3
    assert_eq!(list.len(), 3);

    // Check synthetic result was inserted
    let tool_msg = &list.messages()[1];
    assert_eq!(tool_msg.get("role").and_then(|v| v.as_str()), Some("tool"));
    assert_eq!(
        tool_msg.get("content").and_then(|v| v.as_str()),
        Some(SYNTHETIC_TOOL_RESULT)
    );
}

#[test]
fn test_strict_mode_rejects_orphan() {
    let mut list = ValidatedMessageList::new(None, true);
    list.add_user("hello");

    // No pending tool calls, so adding a tool result should fail in strict mode
    let result = list.add_tool_result("nonexistent", "data");
    assert!(result.is_err());
}

#[test]
fn test_permissive_mode_accepts_orphan() {
    let mut list = ValidatedMessageList::new(None, false);
    list.add_user("hello");

    // Permissive mode: warns but accepts
    let result = list.add_tool_result("nonexistent", "data");
    assert!(result.is_ok());
    assert_eq!(list.len(), 2);
}

#[test]
fn test_batch_tool_results() {
    let mut list = ValidatedMessageList::new(None, false);
    let tcs = vec![tc("tc-1", "bash"), tc("tc-2", "read_file")];
    list.add_assistant(None, Some(tcs.clone()));

    let mut results = std::collections::HashMap::new();
    results.insert("tc-1".to_string(), "output1".to_string());
    // tc-2 is missing — should get synthetic error

    list.add_tool_results_batch(&tcs, &results);
    assert!(!list.has_pending_tools());
    // assistant + 2 tool results = 3
    assert_eq!(list.len(), 3);
}

#[test]
fn test_replace_all_rebuilds_state() {
    let mut list = ValidatedMessageList::new(None, false);
    list.add_user("hello");
    list.add_assistant(None, Some(vec![tc("tc-1", "bash")]));

    // Replace with a clean conversation
    let mut new_msgs = vec![];
    let mut msg = ApiMessage::new();
    msg.insert(
        "role".to_string(),
        serde_json::Value::String("user".to_string()),
    );
    msg.insert(
        "content".to_string(),
        serde_json::Value::String("fresh start".to_string()),
    );
    new_msgs.push(msg);

    list.replace_all(new_msgs);
    assert_eq!(list.len(), 1);
    assert!(!list.has_pending_tools());
}

#[test]
fn test_initial_data_rebuild() {
    // Simulate loading from persisted data with a pending tool call
    let assistant_msg = {
        let mut msg = ApiMessage::new();
        msg.insert(
            "role".to_string(),
            serde_json::Value::String("assistant".to_string()),
        );
        msg.insert(
            "content".to_string(),
            serde_json::Value::String(String::new()),
        );
        msg.insert(
            "tool_calls".to_string(),
            serde_json::json!([{"id": "tc-1", "function": {"name": "bash", "arguments": "{}"}}]),
        );
        msg
    };

    let list = ValidatedMessageList::new(Some(vec![assistant_msg]), false);
    assert!(list.has_pending_tools());
    assert!(list.pending_tool_ids().contains("tc-1"));
}
