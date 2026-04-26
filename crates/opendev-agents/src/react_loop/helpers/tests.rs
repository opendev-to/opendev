use std::collections::HashMap;

use super::super::*;
use crate::prompts::embedded;
use crate::subagents::spec::{PermissionAction, PermissionRule};
use crate::traits::{AgentError, AgentResult, LlmResponse};
fn make_loop() -> ReactLoop {
    ReactLoop::new(ReactLoopConfig {
        max_iterations: Some(10),
        max_nudge_attempts: 3,
        max_todo_nudges: 4,
        ..Default::default()
    })
}

#[test]
fn test_turn_result_equality() {
    assert_eq!(TurnResult::Continue, TurnResult::Continue);
    assert_eq!(TurnResult::Interrupted, TurnResult::Interrupted);
    assert_eq!(TurnResult::MaxIterations, TurnResult::MaxIterations);
    assert_ne!(TurnResult::Continue, TurnResult::Interrupted);
}

#[test]
fn test_process_response_interrupted() {
    let rl = make_loop();
    let resp = LlmResponse::interrupted();
    let result = rl.process_response(&resp, 0);
    assert_eq!(result, TurnResult::Interrupted);
}

#[test]
fn test_process_response_failed() {
    let rl = make_loop();
    let resp = LlmResponse::fail("API error");
    let result = rl.process_response(&resp, 0);
    assert_eq!(result, TurnResult::Continue);
}

#[test]
fn test_process_response_no_tool_calls() {
    let rl = make_loop();
    let msg = serde_json::json!({"role": "assistant", "content": "All done"});
    let resp = LlmResponse::ok(Some("All done".to_string()), msg);
    let result = rl.process_response(&resp, 0);
    match result {
        TurnResult::Complete { content, status } => {
            assert_eq!(content, "All done");
            assert!(status.is_none());
        }
        _ => panic!("Expected Complete"),
    }
}

#[test]
fn test_process_response_with_tool_calls() {
    let rl = make_loop();
    let msg = serde_json::json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{
            "id": "tc-1",
            "function": {"name": "read_file", "arguments": "{}"}
        }]
    });
    let resp = LlmResponse::ok(None, msg);
    let result = rl.process_response(&resp, 0);
    match result {
        TurnResult::ToolCall { tool_calls } => {
            assert_eq!(tool_calls.len(), 1);
        }
        _ => panic!("Expected ToolCall"),
    }
}

#[test]
fn test_all_parallelizable_single_tool() {
    let rl = make_loop();
    let tcs = vec![serde_json::json!({
        "function": {"name": "read_file"}
    })];
    // Single tool is not parallelizable (needs > 1)
    assert!(!rl.all_parallelizable(&tcs));
}

#[test]
fn test_all_parallelizable_multiple_read_only() {
    let rl = make_loop();
    let tcs = vec![
        serde_json::json!({"function": {"name": "Read"}}),
        serde_json::json!({"function": {"name": "Grep"}}),
    ];
    assert!(rl.all_parallelizable(&tcs));
}

#[test]
fn test_all_parallelizable_with_write_tool() {
    let rl = make_loop();
    let tcs = vec![
        serde_json::json!({"function": {"name": "read_file"}}),
        serde_json::json!({"function": {"name": "write_file"}}),
    ];
    assert!(!rl.all_parallelizable(&tcs));
}

#[test]
fn test_all_parallelizable_with_task_complete() {
    let rl = make_loop();
    let tcs = vec![
        serde_json::json!({"function": {"name": "read_file"}}),
        serde_json::json!({"function": {"name": "task_complete"}}),
    ];
    assert!(!rl.all_parallelizable(&tcs));
}

#[test]
fn test_is_task_complete() {
    let tc = serde_json::json!({
        "function": {"name": "task_complete", "arguments": "{}"}
    });
    assert!(ReactLoop::is_task_complete(&tc));

    let tc2 = serde_json::json!({
        "function": {"name": "read_file", "arguments": "{}"}
    });
    assert!(!ReactLoop::is_task_complete(&tc2));
}

#[test]
fn test_extract_task_complete_args() {
    let tc = serde_json::json!({
        "function": {
            "name": "task_complete",
            "arguments": "{\"result\": \"All done\", \"status\": \"success\"}"
        }
    });
    let (summary, status) = ReactLoop::extract_task_complete_args(&tc);
    assert_eq!(summary, "All done");
    assert_eq!(status, "success");
}

#[test]
fn test_extract_task_complete_args_defaults() {
    let tc = serde_json::json!({
        "function": {"name": "task_complete", "arguments": "{}"}
    });
    let (summary, status) = ReactLoop::extract_task_complete_args(&tc);
    assert_eq!(summary, "Task completed");
    assert_eq!(status, "success");
}

#[test]
fn test_format_tool_result_success() {
    let result = serde_json::json!({"success": true, "output": "file contents"});
    let formatted = ReactLoop::format_tool_result("read_file", &result);
    assert_eq!(formatted, "file contents");
}

#[test]
fn test_format_tool_result_success_with_status() {
    let result = serde_json::json!({
        "success": true,
        "output": "done",
        "completion_status": "partial"
    });
    let formatted = ReactLoop::format_tool_result("write_file", &result);
    assert_eq!(formatted, "[completion_status=partial]\ndone");
}

#[test]
fn test_format_tool_result_failure() {
    let result = serde_json::json!({"success": false, "error": "file not found"});
    let formatted = ReactLoop::format_tool_result("read_file", &result);
    assert_eq!(formatted, "Error: file not found");
}

#[test]
fn test_classify_error_permission() {
    assert_eq!(
        ReactLoop::classify_error("Permission denied: /etc"),
        "permission_error"
    );
}

#[test]
fn test_classify_error_edit_mismatch() {
    assert_eq!(
        ReactLoop::classify_error("old_content not found in file"),
        "edit_mismatch"
    );
}

#[test]
fn test_classify_error_file_not_found() {
    assert_eq!(
        ReactLoop::classify_error("No such file or directory"),
        "file_not_found"
    );
}

#[test]
fn test_classify_error_syntax() {
    assert_eq!(
        ReactLoop::classify_error("SyntaxError: unexpected token"),
        "syntax_error"
    );
}

#[test]
fn test_classify_error_rate_limit() {
    assert_eq!(
        ReactLoop::classify_error("429 Too Many Requests"),
        "rate_limit"
    );
}

#[test]
fn test_classify_error_timeout() {
    assert_eq!(ReactLoop::classify_error("Request timed out"), "timeout");
}

#[test]
fn test_classify_error_generic() {
    assert_eq!(ReactLoop::classify_error("Something went wrong"), "generic");
}

#[test]
fn test_check_iteration_limit_unlimited() {
    let rl = ReactLoop::new(ReactLoopConfig {
        max_iterations: None,
        ..Default::default()
    });
    assert!(!rl.check_iteration_limit(1));
    assert!(!rl.check_iteration_limit(1000));
}

#[test]
fn test_check_iteration_limit_bounded() {
    let rl = make_loop();
    assert!(!rl.check_iteration_limit(10)); // At limit
    assert!(rl.check_iteration_limit(11)); // Over limit
}

#[test]
fn test_process_iteration_max_iterations() {
    let rl = make_loop();
    let resp = LlmResponse::ok(Some("hello".into()), serde_json::json!({}));
    let mut messages = vec![];
    let mut no_tools = 0;
    let result = rl.process_iteration(&resp, &mut messages, 11, &mut no_tools);
    assert!(matches!(result, Ok(TurnResult::MaxIterations)));
}

#[test]
fn test_process_iteration_interrupted() {
    let rl = make_loop();
    let resp = LlmResponse::interrupted();
    let mut messages = vec![];
    let mut no_tools = 0;
    let result = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
    assert!(matches!(result, Ok(TurnResult::Interrupted)));
}

#[test]
fn test_process_iteration_failed() {
    let rl = make_loop();
    let resp = LlmResponse::fail("error");
    let mut messages = vec![];
    let mut no_tools = 0;
    let result = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
    assert!(matches!(result, Err(AgentError::LlmError(_))));
}

#[test]
fn test_process_iteration_appends_message() {
    let rl = make_loop();
    let msg = serde_json::json!({"role": "assistant", "content": "hi"});
    let resp = LlmResponse::ok(Some("hi".into()), msg);
    let mut messages = vec![];
    let mut no_tools = 0;
    let _ = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "assistant");
}

#[test]
fn test_process_iteration_increments_no_tool_counter() {
    let rl = make_loop();
    let msg = serde_json::json!({"role": "assistant", "content": "done"});
    let resp = LlmResponse::ok(Some("done".into()), msg);
    let mut messages = vec![];
    let mut no_tools = 0;
    let _ = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
    assert_eq!(no_tools, 1);
}

#[test]
fn test_process_iteration_resets_no_tool_counter_on_tool_call() {
    let rl = make_loop();
    let msg = serde_json::json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{"id": "1", "function": {"name": "read_file", "arguments": "{}"}}]
    });
    let resp = LlmResponse::ok(None, msg);
    let mut messages = vec![];
    let mut no_tools = 5;
    let _ = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
    assert_eq!(no_tools, 0);
}

#[test]
fn test_default_config() {
    let config = ReactLoopConfig::default();
    assert_eq!(config.max_iterations, Some(50));
    assert_eq!(config.max_nudge_attempts, 3);
    assert_eq!(config.max_todo_nudges, 4);
}

// --- Shallow subagent detection tests ---

#[test]
fn test_shallow_subagent_no_tools() {
    let messages = vec![
        serde_json::json!({"role": "system", "content": "You are..."}),
        serde_json::json!({"role": "user", "content": "do something"}),
        serde_json::json!({"role": "assistant", "content": "Done without tools."}),
    ];
    assert_eq!(ReactLoop::count_subagent_tool_calls(&messages), 0);
    let warning = ReactLoop::shallow_subagent_warning(&messages, true);
    assert!(warning.is_some());
    assert!(warning.unwrap().contains("SHALLOW SUBAGENT WARNING"));
}

#[test]
fn test_shallow_subagent_one_tool() {
    let messages = vec![
        serde_json::json!({"role": "assistant", "content": null, "tool_calls": [
            {"id": "1", "function": {"name": "read_file", "arguments": "{}"}}
        ]}),
        serde_json::json!({"role": "tool", "name": "read_file", "content": "ok"}),
        serde_json::json!({"role": "assistant", "content": "Here is the file."}),
    ];
    assert_eq!(ReactLoop::count_subagent_tool_calls(&messages), 1);
    assert!(ReactLoop::shallow_subagent_warning(&messages, true).is_some());
}

#[test]
fn test_not_shallow_subagent_many_tools() {
    let messages = vec![
        serde_json::json!({"role": "assistant", "content": null, "tool_calls": [
            {"id": "1", "function": {"name": "read_file", "arguments": "{}"}}
        ]}),
        serde_json::json!({"role": "tool", "name": "read_file", "content": "ok"}),
        serde_json::json!({"role": "assistant", "content": null, "tool_calls": [
            {"id": "2", "function": {"name": "edit_file", "arguments": "{}"}}
        ]}),
        serde_json::json!({"role": "tool", "name": "edit_file", "content": "ok"}),
        serde_json::json!({"role": "assistant", "content": "Done."}),
    ];
    assert_eq!(ReactLoop::count_subagent_tool_calls(&messages), 2);
    assert!(ReactLoop::shallow_subagent_warning(&messages, true).is_none());
}

#[test]
fn test_count_subagent_tool_calls_parallel() {
    // One assistant turn with 3 parallel tool calls should count as 3
    let messages = vec![
        serde_json::json!({"role": "assistant", "content": null, "tool_calls": [
            {"id": "1", "function": {"name": "read_file", "arguments": "{}"}},
            {"id": "2", "function": {"name": "read_file", "arguments": "{}"}},
            {"id": "3", "function": {"name": "read_file", "arguments": "{}"}}
        ]}),
        serde_json::json!({"role": "tool", "name": "read_file", "content": "ok"}),
        serde_json::json!({"role": "tool", "name": "read_file", "content": "ok"}),
        serde_json::json!({"role": "tool", "name": "read_file", "content": "ok"}),
        serde_json::json!({"role": "assistant", "content": "Done."}),
    ];
    // 3 individual tool calls
    assert_eq!(ReactLoop::count_subagent_tool_calls(&messages), 3);
    // But only 1 tool turn
    assert_eq!(ReactLoop::count_subagent_tool_turns(&messages), 1);
    // Shallow warning triggers (only 1 turn)
    assert!(ReactLoop::shallow_subagent_warning(&messages, true).is_some());
}

#[test]
fn test_shallow_subagent_failed_no_warning() {
    let messages = vec![serde_json::json!({"role": "assistant", "content": "I failed."})];
    assert!(ReactLoop::shallow_subagent_warning(&messages, false).is_none());
}

// --- Iteration metrics tests ---

#[test]
fn test_iteration_metrics_default() {
    let metrics = IterationMetrics::default();
    assert_eq!(metrics.iteration, 0);
    assert_eq!(metrics.llm_latency_ms, 0);
    assert_eq!(metrics.input_tokens, 0);
    assert_eq!(metrics.output_tokens, 0);
    assert!(metrics.tool_calls.is_empty());
    assert_eq!(metrics.total_duration_ms, 0);
}

#[test]
fn test_tool_call_metric() {
    let metric = ToolCallMetric {
        tool_name: "read_file".to_string(),
        duration_ms: 42,
        success: true,
    };
    assert_eq!(metric.tool_name, "read_file");
    assert_eq!(metric.duration_ms, 42);
    assert!(metric.success);
}

#[test]
fn test_metrics_accumulation() {
    let rl = make_loop();

    // Initially empty
    assert!(rl.iteration_metrics().is_empty());

    // Push a metric
    rl.push_metrics(IterationMetrics {
        iteration: 1,
        llm_latency_ms: 100,
        input_tokens: 500,
        output_tokens: 200,
        tool_calls: vec![ToolCallMetric {
            tool_name: "read_file".to_string(),
            duration_ms: 10,
            success: true,
        }],
        total_duration_ms: 150,
    });

    let metrics = rl.iteration_metrics();
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].iteration, 1);
    assert_eq!(metrics[0].llm_latency_ms, 100);
    assert_eq!(metrics[0].tool_calls.len(), 1);
    assert_eq!(metrics[0].tool_calls[0].tool_name, "read_file");
}

#[test]
fn test_metrics_clear() {
    let rl = make_loop();
    rl.push_metrics(IterationMetrics {
        iteration: 1,
        ..Default::default()
    });
    assert_eq!(rl.iteration_metrics().len(), 1);

    rl.clear_metrics();
    assert!(rl.iteration_metrics().is_empty());
}

// --- Partial result preservation tests ---

#[test]
fn test_agent_result_interrupted_with_partial_content() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "do stuff"}),
        serde_json::json!({"role": "tool", "name": "read_file", "content": "file data", "tool_call_id": "tc-1"}),
    ];
    let mut result = AgentResult::interrupted(messages);
    // Simulate partial content preservation
    result.content = "Task interrupted by user (partial): I was analyzing...".to_string();
    assert!(result.interrupted);
    assert!(result.content.contains("partial"));
    assert!(result.content.contains("analyzing"));
    // Messages should include the completed tool result
    assert_eq!(result.messages.len(), 2);
    assert_eq!(result.messages[1]["name"], "read_file");
}

// --- Permission enforcement tests ---

#[test]
fn test_evaluate_permission_empty_rules() {
    let config = ReactLoopConfig::default();
    assert!(config.evaluate_permission("run_command", "ls").is_none());
}

#[test]
fn test_evaluate_permission_blanket_deny() {
    let mut config = ReactLoopConfig::default();
    config.permission.insert(
        "run_command".to_string(),
        PermissionRule::Action(PermissionAction::Deny),
    );
    assert_eq!(
        config.evaluate_permission("run_command", "ls"),
        Some(PermissionAction::Deny)
    );
}

#[test]
fn test_evaluate_permission_blanket_allow() {
    let mut config = ReactLoopConfig::default();
    config.permission.insert(
        "read_file".to_string(),
        PermissionRule::Action(PermissionAction::Allow),
    );
    assert_eq!(
        config.evaluate_permission("read_file", ""),
        Some(PermissionAction::Allow)
    );
}

#[test]
fn test_evaluate_permission_wildcard_tool_pattern() {
    let mut config = ReactLoopConfig::default();
    config.permission.insert(
        "*".to_string(),
        PermissionRule::Action(PermissionAction::Ask),
    );
    assert_eq!(
        config.evaluate_permission("write_file", ""),
        Some(PermissionAction::Ask)
    );
}

#[test]
fn test_evaluate_permission_specific_overrides_wildcard() {
    let mut config = ReactLoopConfig::default();
    config.permission.insert(
        "*".to_string(),
        PermissionRule::Action(PermissionAction::Ask),
    );
    config.permission.insert(
        "read_file".to_string(),
        PermissionRule::Action(PermissionAction::Allow),
    );
    // Specific "read_file" should override wildcard "*"
    assert_eq!(
        config.evaluate_permission("read_file", ""),
        Some(PermissionAction::Allow)
    );
    // Wildcard still applies to other tools
    assert_eq!(
        config.evaluate_permission("write_file", ""),
        Some(PermissionAction::Ask)
    );
}

#[test]
fn test_evaluate_permission_pattern_rules() {
    let mut patterns = HashMap::new();
    patterns.insert("*".to_string(), PermissionAction::Ask);
    patterns.insert("git *".to_string(), PermissionAction::Allow);
    patterns.insert("rm -rf *".to_string(), PermissionAction::Deny);

    let mut config = ReactLoopConfig::default();
    config.permission.insert(
        "run_command".to_string(),
        PermissionRule::Patterns(patterns),
    );

    assert_eq!(
        config.evaluate_permission("run_command", "git status"),
        Some(PermissionAction::Allow)
    );
    assert_eq!(
        config.evaluate_permission("run_command", "rm -rf /"),
        Some(PermissionAction::Deny)
    );
    assert_eq!(
        config.evaluate_permission("run_command", "ls -la"),
        Some(PermissionAction::Ask)
    );
}

#[test]
fn test_evaluate_permission_no_match() {
    let mut config = ReactLoopConfig::default();
    config.permission.insert(
        "run_command".to_string(),
        PermissionRule::Action(PermissionAction::Deny),
    );
    // Different tool should not match
    assert!(config.evaluate_permission("read_file", "").is_none());
}

#[test]
fn test_default_config_has_empty_permissions() {
    let config = ReactLoopConfig::default();
    assert!(config.permission.is_empty());
}

#[test]
fn test_mcp_tool_needs_approval_gate() {
    // MCP tools (mcp__*) should be treated the same as run_command
    // for approval purposes
    let tool_name = "mcp__github__create_issue";
    let needs_approval = tool_name == "run_command" || tool_name.starts_with("mcp__");
    assert!(needs_approval);

    // Regular tools should not need the bash approval gate
    let tool_name = "read_file";
    let needs_approval = tool_name == "run_command" || tool_name.starts_with("mcp__");
    assert!(!needs_approval);
}

#[test]
fn test_mcp_permission_rule_matches() {
    let mut config = ReactLoopConfig::default();
    config.permission.insert(
        "mcp__*".to_string(),
        PermissionRule::Action(PermissionAction::Ask),
    );
    // MCP tool should match the glob
    assert_eq!(
        config.evaluate_permission("mcp__sqlite__query", ""),
        Some(PermissionAction::Ask)
    );
    // Non-MCP tool should not match
    assert!(config.evaluate_permission("read_file", "").is_none());
}

#[test]
fn test_mcp_permission_allow_specific() {
    let mut config = ReactLoopConfig::default();
    // Deny all MCP by default
    config.permission.insert(
        "mcp__*".to_string(),
        PermissionRule::Action(PermissionAction::Deny),
    );
    // But allow a specific MCP tool
    config.permission.insert(
        "mcp__sqlite__query".to_string(),
        PermissionRule::Action(PermissionAction::Allow),
    );
    // Specific rule should win (higher specificity)
    assert_eq!(
        config.evaluate_permission("mcp__sqlite__query", ""),
        Some(PermissionAction::Allow)
    );
    // Other MCP tools should be denied
    assert_eq!(
        config.evaluate_permission("mcp__github__push", ""),
        Some(PermissionAction::Deny)
    );
}
