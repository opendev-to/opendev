use super::*;

// --- HandoffMessage tests ---

#[test]
fn test_handoff_create() {
    let from = AgentDefinition::from_role(AgentRole::Code);
    let messages = vec![
        serde_json::json!({"role": "user", "content": "implement feature X"}),
        serde_json::json!({"role": "assistant", "content": "I read the file and found..."}),
        serde_json::json!({
            "role": "tool",
            "name": "read_file",
            "content": "fn main() { println!(\"hello\"); }",
            "tool_call_id": "tc-1"
        }),
        serde_json::json!({"role": "assistant", "content": "Done with initial analysis."}),
    ];
    let handoff = HandoffMessage::create_handoff(&from, &AgentRole::Test, &messages);
    assert_eq!(handoff.from_agent, "Code");
    assert_eq!(handoff.to_agent, "Test");
    assert_eq!(handoff.summary, "Done with initial analysis.");
    assert!(!handoff.key_findings.is_empty());
    assert!(handoff.pending_actions.is_empty());
}

#[test]
fn test_handoff_with_errors() {
    let from = AgentDefinition::from_role(AgentRole::Build);
    let messages = vec![
        serde_json::json!({"role": "assistant", "content": "Trying to fix..."}),
        serde_json::json!({
            "role": "tool",
            "name": "bash",
            "content": "Error in bash: compilation failed",
            "tool_call_id": "tc-1"
        }),
    ];
    let handoff = HandoffMessage::create_handoff(&from, &AgentRole::Code, &messages);
    assert!(!handoff.pending_actions.is_empty());
    assert!(handoff.pending_actions[0].contains("Retry bash"));
}

#[test]
fn test_handoff_empty_messages() {
    let from = AgentDefinition::from_role(AgentRole::Code);
    let handoff = HandoffMessage::create_handoff(&from, &AgentRole::Plan, &[]);
    assert_eq!(handoff.summary, "No summary available");
    assert!(handoff.key_findings.is_empty());
    assert!(handoff.pending_actions.is_empty());
}

#[test]
fn test_handoff_to_context_message() {
    let handoff = HandoffMessage {
        from_agent: "Code".into(),
        to_agent: "Test".into(),
        summary: "Implemented feature X".into(),
        key_findings: vec!["Found bug in parser".into()],
        pending_actions: vec!["Write unit tests".into()],
    };
    let msg = handoff.to_context_message();
    assert_eq!(msg["role"], "user");
    let content = msg["content"].as_str().unwrap();
    assert!(content.contains("[HANDOFF from Code agent]"));
    assert!(content.contains("Implemented feature X"));
    assert!(content.contains("Found bug in parser"));
    assert!(content.contains("Write unit tests"));
}

#[test]
fn test_handoff_to_context_message_empty_findings() {
    let handoff = HandoffMessage {
        from_agent: "Plan".into(),
        to_agent: "Code".into(),
        summary: "Plan complete".into(),
        key_findings: vec![],
        pending_actions: vec![],
    };
    let msg = handoff.to_context_message();
    let content = msg["content"].as_str().unwrap();
    assert!(content.contains("None"));
}

#[test]
fn test_handoff_message_serialization() {
    let handoff = HandoffMessage {
        from_agent: "Code".into(),
        to_agent: "Test".into(),
        summary: "Done".into(),
        key_findings: vec!["found a bug".into()],
        pending_actions: vec!["write tests".into()],
    };
    let json = serde_json::to_string(&handoff).unwrap();
    let roundtrip: HandoffMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.from_agent, "Code");
    assert_eq!(roundtrip.key_findings.len(), 1);
}

// --- can_parallelize tests ---

#[test]
fn test_can_parallelize_single_call() {
    let calls = vec![serde_json::json!({
        "function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}
    })];
    let groups = can_parallelize(&calls);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 1);
}

#[test]
fn test_can_parallelize_different_files() {
    let calls = vec![
        serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}}),
        serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"b.rs\"}"}}),
        serde_json::json!({"function": {"name": "edit_file", "arguments": "{\"path\": \"c.rs\"}"}}),
    ];
    let groups = can_parallelize(&calls);
    assert_eq!(groups.len(), 3);
    assert!(groups.iter().all(|g| g.len() == 1));
}

#[test]
fn test_can_parallelize_same_file() {
    let calls = vec![
        serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}}),
        serde_json::json!({"function": {"name": "edit_file", "arguments": "{\"path\": \"a.rs\"}"}}),
    ];
    let groups = can_parallelize(&calls);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 2);
}

#[test]
fn test_can_parallelize_mixed() {
    let calls = vec![
        serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}}),
        serde_json::json!({"function": {"name": "edit_file", "arguments": "{\"path\": \"a.rs\"}"}}),
        serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"b.rs\"}"}}),
    ];
    let groups = can_parallelize(&calls);
    assert_eq!(groups.len(), 2);
}

#[test]
fn test_can_parallelize_no_path() {
    let calls = vec![
        serde_json::json!({"function": {"name": "bash", "arguments": "{\"command\": \"cargo test\"}"}}),
        serde_json::json!({"function": {"name": "bash", "arguments": "{\"command\": \"cargo build\"}"}}),
    ];
    let groups = can_parallelize(&calls);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 2);
}

#[test]
fn test_can_parallelize_mixed_path_and_no_path() {
    let calls = vec![
        serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}}),
        serde_json::json!({"function": {"name": "bash", "arguments": "{\"command\": \"ls\"}"}}),
    ];
    let groups = can_parallelize(&calls);
    assert_eq!(groups.len(), 2);
}

#[test]
fn test_can_parallelize_empty() {
    let groups = can_parallelize(&[]);
    assert_eq!(groups.len(), 1);
    assert!(groups[0].is_empty());
}

#[test]
fn test_can_parallelize_file_path_key() {
    let calls = vec![
        serde_json::json!({"function": {"name": "write_file", "arguments": "{\"file_path\": \"x.rs\"}"}}),
        serde_json::json!({"function": {"name": "write_file", "arguments": "{\"file_path\": \"y.rs\"}"}}),
    ];
    let groups = can_parallelize(&calls);
    assert_eq!(groups.len(), 2);
}

// --- extract_file_path tests ---

#[test]
fn test_extract_file_path_with_path() {
    let tc = serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"src/main.rs\"}"}});
    assert_eq!(extract_file_path(&tc), Some("src/main.rs".to_string()));
}

#[test]
fn test_extract_file_path_with_file_path() {
    let tc = serde_json::json!({"function": {"name": "write_file", "arguments": "{\"file_path\": \"out.txt\"}"}});
    assert_eq!(extract_file_path(&tc), Some("out.txt".to_string()));
}

#[test]
fn test_extract_file_path_with_file() {
    let tc =
        serde_json::json!({"function": {"name": "edit", "arguments": "{\"file\": \"lib.rs\"}"}});
    assert_eq!(extract_file_path(&tc), Some("lib.rs".to_string()));
}

#[test]
fn test_extract_file_path_none() {
    let tc =
        serde_json::json!({"function": {"name": "bash", "arguments": "{\"command\": \"ls\"}"}});
    assert_eq!(extract_file_path(&tc), None);
}

// --- PartialResult tests ---

#[test]
fn test_partial_result_from_interrupted_state() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "do stuff"}),
        serde_json::json!({
            "role": "assistant",
            "content": "I'll read the files.",
            "tool_calls": [{"id": "tc-1", "function": {"name": "read_file", "arguments": "{}"}}]
        }),
        serde_json::json!({"role": "tool", "name": "read_file", "content": "file contents", "tool_call_id": "tc-1"}),
        serde_json::json!({"role": "tool", "name": "search", "content": "search results", "tool_call_id": "tc-2"}),
    ];
    let partial =
        PartialResult::from_interrupted_state(&messages, Some("I was analyzing..."), 3, 2, 5);
    assert_eq!(partial.completed_tool_results.len(), 2);
    assert_eq!(
        partial.last_assistant_content.as_deref(),
        Some("I was analyzing...")
    );
    assert_eq!(partial.interrupted_at_iteration, 3);
    assert_eq!(partial.completed_tool_count, 2);
    assert_eq!(partial.total_tool_count, 5);
}

#[test]
fn test_partial_result_summary() {
    let partial = PartialResult {
        completed_tool_results: vec![serde_json::json!({"role": "tool", "content": "ok"})],
        last_assistant_content: None,
        interrupted_at_iteration: 5,
        completed_tool_count: 1,
        total_tool_count: 3,
    };
    let summary = partial.summary();
    assert!(summary.contains("iteration 5"));
    assert!(summary.contains("1/3"));
    assert!(summary.contains("1 tool result(s) preserved"));
}

#[test]
fn test_partial_result_empty() {
    let partial = PartialResult::from_interrupted_state(&[], None, 1, 0, 0);
    assert!(partial.completed_tool_results.is_empty());
    assert!(partial.last_assistant_content.is_none());
    assert_eq!(
        partial.summary(),
        "Interrupted at iteration 1 (0/0 tool calls completed). 0 tool result(s) preserved."
    );
}

#[test]
fn test_partial_result_serialization() {
    let partial = PartialResult {
        completed_tool_results: vec![serde_json::json!({"role": "tool"})],
        last_assistant_content: Some("partial".into()),
        interrupted_at_iteration: 2,
        completed_tool_count: 1,
        total_tool_count: 3,
    };
    let json = serde_json::to_string(&partial).unwrap();
    let roundtrip: PartialResult = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.interrupted_at_iteration, 2);
    assert_eq!(roundtrip.last_assistant_content.as_deref(), Some("partial"));
}
