use super::*;

#[test]
fn test_simple_runner_parse_tool_calls_empty() {
    let body = serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Hello"
            }
        }]
    });
    let calls = SimpleReactRunner::parse_tool_calls(&body);
    assert!(calls.is_empty());
}

#[test]
fn test_simple_runner_parse_tool_calls_present() {
    let body = serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "tc-1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"file_path\": \"/src/main.rs\"}"
                    }
                }]
            }
        }]
    });
    let calls = SimpleReactRunner::parse_tool_calls(&body);
    assert_eq!(calls.len(), 1);
}

#[test]
fn test_simple_runner_parse_content() {
    let body = serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Here is the analysis."
            }
        }]
    });
    let content = SimpleReactRunner::parse_content(&body);
    assert_eq!(content.as_deref(), Some("Here is the analysis."));
}

#[test]
fn test_simple_runner_parse_token_usage() {
    let body = serde_json::json!({
        "usage": {
            "prompt_tokens": 1000,
            "completion_tokens": 500
        }
    });
    let (input, output) = SimpleReactRunner::parse_token_usage(&body);
    assert_eq!(input, 1000);
    assert_eq!(output, 500);
}

#[test]
fn test_simple_runner_extract_tool_info() {
    let tc = serde_json::json!({
        "id": "call_abc",
        "type": "function",
        "function": {
            "name": "read_file",
            "arguments": "{\"file_path\": \"/src/main.rs\"}"
        }
    });
    let (id, name, args) = SimpleReactRunner::extract_tool_info(&tc);
    assert_eq!(id, "call_abc");
    assert_eq!(name, "read_file");
    assert_eq!(
        args.get("file_path").and_then(|v| v.as_str()),
        Some("/src/main.rs")
    );
}

#[test]
fn test_build_exploration_observation() {
    let messages = vec![
        serde_json::json!({"role": "system", "content": "You are an explorer."}),
        serde_json::json!({"role": "user", "content": "Explore the codebase"}),
        serde_json::json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {"id": "1", "function": {"name": "list_files", "arguments": "{\"path\": \"src\"}"}},
                {"id": "2", "function": {"name": "read_file", "arguments": "{\"file_path\": \"src/main.rs\"}"}},
                {"id": "3", "function": {"name": "read_file", "arguments": "{\"file_path\": \"Cargo.toml\"}"}}
            ]
        }),
        serde_json::json!({"role": "tool", "tool_call_id": "1", "content": "file list"}),
        serde_json::json!({"role": "tool", "tool_call_id": "2", "content": "fn main(){}"}),
        serde_json::json!({"role": "tool", "tool_call_id": "3", "content": "[package]"}),
        serde_json::json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {"id": "4", "function": {"name": "search", "arguments": "{\"pattern\": \"fn main\"}"}}
            ]
        }),
        serde_json::json!({"role": "tool", "tool_call_id": "4", "content": "matches"}),
    ];

    let obs =
        SimpleReactRunner::build_exploration_observation(&messages, "Explore the codebase");
    assert!(obs.contains("**Original task**: Explore the codebase"));
    assert!(obs.contains("4 tool calls"));
    assert!(obs.contains("Files read (2)"));
    assert!(obs.contains("src/main.rs"));
    assert!(obs.contains("Cargo.toml"));
    assert!(obs.contains("Searches (1)"));
    assert!(obs.contains("`fn main`"));
    assert!(obs.contains("Directories listed (1)"));
    assert!(obs.contains("src"));
    // Should not contain commands since none were run
    assert!(!obs.contains("Commands run"));
}

#[test]
fn test_build_exploration_observation_deduplicates_files() {
    let messages = vec![serde_json::json!({
        "role": "assistant",
        "tool_calls": [
            {"id": "1", "function": {"name": "read_file", "arguments": "{\"file_path\": \"src/main.rs\"}"}},
            {"id": "2", "function": {"name": "read_file", "arguments": "{\"file_path\": \"src/main.rs\"}"}}
        ]
    })];
    let obs = SimpleReactRunner::build_exploration_observation(&messages, "test");
    assert!(obs.contains("Files read (1)"));
}

#[test]
fn test_simple_runner_name() {
    let runner = SimpleReactRunner::new(50);
    assert_eq!(runner.name(), "SimpleReactRunner");
}
