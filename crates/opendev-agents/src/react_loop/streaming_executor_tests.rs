//! Tests for the streaming tool executor.

use super::*;
use std::sync::Arc;

/// Test that FunctionCallStart events are stored by index.
#[test]
fn test_function_call_start_stored() {
    let registry = Arc::new(ToolRegistry::new());
    let context = ToolContext {
        working_dir: std::path::PathBuf::from("/tmp"),
        ..ToolContext::default()
    };
    let executor = StreamingToolExecutor::new(registry, context, None);

    // Send a FunctionCallStart event
    executor.on_event(&StreamEvent::FunctionCallStart {
        index: 0,
        call_id: "call_123".to_string(),
        name: "Read".to_string(),
    });

    // Verify it was stored by index
    let map = executor.call_metadata.lock().unwrap();
    assert_eq!(map.len(), 1);
    assert_eq!(map[&0].call_id, "call_123");
    assert_eq!(map[&0].name, "Read");
}

/// Test that write tools get pre-parsed args instead of early execution.
#[test]
fn test_write_tool_preparsed() {
    let registry = Arc::new(ToolRegistry::new());
    let context = ToolContext {
        working_dir: std::path::PathBuf::from("/tmp"),
        ..ToolContext::default()
    };
    let executor = StreamingToolExecutor::new(registry, context, None);

    // Queue a write tool
    executor.on_event(&StreamEvent::FunctionCallStart {
        index: 0,
        call_id: "call_456".to_string(),
        name: "Edit".to_string(),
    });

    // Complete it
    executor.on_event(&StreamEvent::FunctionCallDone {
        index: 0,
        arguments: r#"{"file_path": "/tmp/test.rs", "old_string": "foo", "new_string": "bar"}"#
            .to_string(),
    });

    // Should have pre-parsed args, not an early result
    assert!(executor.take_result("call_456").is_none());
    let preparsed = executor.take_preparsed_args("call_456");
    assert!(preparsed.is_some());
    let args = preparsed.unwrap().args_map;

    // Normalize separators to be OS-agnostic on Windows vs Unix
    let got = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .map(|s| s.replace('\\', "/"));
    assert_eq!(got.as_deref(), Some("/tmp/test.rs"));
}

/// Test that non-matching events are ignored.
#[test]
fn test_ignores_irrelevant_events() {
    let registry = Arc::new(ToolRegistry::new());
    let context = ToolContext {
        working_dir: std::path::PathBuf::from("/tmp"),
        ..ToolContext::default()
    };
    let executor = StreamingToolExecutor::new(registry, context, None);

    executor.on_event(&StreamEvent::TextDelta("hello".to_string()));
    executor.on_event(&StreamEvent::ReasoningDelta("thinking".to_string()));
    executor.on_event(&StreamEvent::Error("oops".to_string()));

    assert!(!executor.has_results());
    assert!(!executor.has_running_tasks());
}

/// Test that has_results returns false initially.
#[test]
fn test_initial_state() {
    let registry = Arc::new(ToolRegistry::new());
    let context = ToolContext {
        working_dir: std::path::PathBuf::from("/tmp"),
        ..ToolContext::default()
    };
    let executor = StreamingToolExecutor::new(registry, context, None);

    assert!(!executor.has_results());
    assert!(!executor.has_running_tasks());
}

/// Test that read-only tool spawns a task (async test).
#[tokio::test]
async fn test_read_only_tool_spawns_task() {
    let registry = Arc::new(ToolRegistry::new());
    let context = ToolContext {
        working_dir: std::path::PathBuf::from("/tmp"),
        ..ToolContext::default()
    };
    let executor = StreamingToolExecutor::new(registry, context, None);

    // Queue a read-only tool
    executor.on_event(&StreamEvent::FunctionCallStart {
        index: 0,
        call_id: "call_789".to_string(),
        name: "Read".to_string(),
    });

    // Complete it - this should spawn a task
    executor.on_event(&StreamEvent::FunctionCallDone {
        index: 0,
        arguments: r#"{"file_path": "/tmp/nonexistent_test_file.txt"}"#.to_string(),
    });

    // A task should have been spawned
    assert!(executor.has_running_tasks());

    // Wait for completion
    executor.wait_for_completion().await;

    // Should have a result now (probably an error since file doesn't exist,
    // but it demonstrates the tool was executed)
    assert!(executor.has_results());
}
