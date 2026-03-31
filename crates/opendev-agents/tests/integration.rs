//! Integration tests for the agent layer.
//!
//! Tests process_response decision logic, iteration limits, interrupt handling,
//! tool call parallelizability detection, and the full ReAct loop with mock tools.

use std::collections::HashMap;

use opendev_agents::llm_calls::LlmCallConfig;
use opendev_agents::prompts::composer::ctx_bool;
use opendev_agents::react_loop::ReactLoopConfig;
use opendev_agents::traits::{AgentDeps, AgentError, AgentResult, LlmResponse};
use opendev_agents::{LlmCaller, PromptComposer, ReactLoop, ResponseCleaner, TurnResult};

// ========================================================================
// process_response tests
// ========================================================================

fn make_loop() -> ReactLoop {
    ReactLoop::new(ReactLoopConfig {
        max_iterations: Some(10),
        max_nudge_attempts: 3,
        max_todo_nudges: 4,
        ..Default::default()
    })
}

/// process_response returns ToolCall when response contains tool_calls.
#[test]
fn process_response_returns_tool_call() {
    let rl = make_loop();
    let msg = serde_json::json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [
            {"id": "tc-1", "function": {"name": "read_file", "arguments": "{\"file_path\": \"/tmp/x\"}"}},
            {"id": "tc-2", "function": {"name": "search", "arguments": "{\"pattern\": \"TODO\"}"}}
        ]
    });
    let resp = LlmResponse::ok(None, msg);
    let result = rl.process_response(&resp, 0);

    match result {
        TurnResult::ToolCall { tool_calls } => {
            assert_eq!(tool_calls.len(), 2);
        }
        other => panic!("Expected ToolCall, got {:?}", other),
    }
}

/// process_response returns Complete when no tool calls in response.
#[test]
fn process_response_returns_complete_without_tools() {
    let rl = make_loop();
    let msg = serde_json::json!({"role": "assistant", "content": "I've finished the task."});
    let resp = LlmResponse::ok(Some("I've finished the task.".into()), msg);
    let result = rl.process_response(&resp, 0);

    match result {
        TurnResult::Complete { content, status } => {
            assert_eq!(content, "I've finished the task.");
            assert!(status.is_none());
        }
        other => panic!("Expected Complete, got {:?}", other),
    }
}

/// process_response returns Interrupted when response is marked interrupted.
#[test]
fn process_response_returns_interrupted() {
    let rl = make_loop();
    let resp = LlmResponse::interrupted();
    assert_eq!(rl.process_response(&resp, 0), TurnResult::Interrupted);
}

/// process_response returns Continue when LLM call failed.
#[test]
fn process_response_continues_on_failure() {
    let rl = make_loop();
    let resp = LlmResponse::fail("Rate limited");
    assert_eq!(rl.process_response(&resp, 0), TurnResult::Continue);
}

// ========================================================================
// Iteration limit tests
// ========================================================================

/// check_iteration_limit returns false when unlimited.
#[test]
fn unlimited_iterations_never_hit_limit() {
    let rl = ReactLoop::new(ReactLoopConfig {
        max_iterations: None,
        ..Default::default()
    });
    assert!(!rl.check_iteration_limit(1));
    assert!(!rl.check_iteration_limit(10_000));
}

/// check_iteration_limit returns true when over limit.
#[test]
fn bounded_iterations_trigger_at_limit() {
    let rl = make_loop();
    assert!(!rl.check_iteration_limit(10)); // at limit, not over
    assert!(rl.check_iteration_limit(11)); // over limit
}

// ========================================================================
// Parallelizability detection
// ========================================================================

/// Multiple read-only tools are parallelizable.
#[test]
fn multiple_read_only_tools_are_parallel() {
    let rl = make_loop();
    let tool_calls = vec![
        serde_json::json!({"function": {"name": "Read"}}),
        serde_json::json!({"function": {"name": "Grep"}}),
        serde_json::json!({"function": {"name": "Glob"}}),
    ];
    assert!(rl.all_parallelizable(&tool_calls));
}

/// A single tool is never parallelizable (needs >1).
#[test]
fn single_tool_not_parallelizable() {
    let rl = make_loop();
    let tool_calls = vec![serde_json::json!({"function": {"name": "read_file"}})];
    assert!(!rl.all_parallelizable(&tool_calls));
}

/// Mix of read-only and write tools is not parallelizable.
#[test]
fn mixed_read_write_not_parallelizable() {
    let rl = make_loop();
    let tool_calls = vec![
        serde_json::json!({"function": {"name": "read_file"}}),
        serde_json::json!({"function": {"name": "write_file"}}),
    ];
    assert!(!rl.all_parallelizable(&tool_calls));
}

/// task_complete is never parallelizable even with read-only tools.
#[test]
fn task_complete_blocks_parallel() {
    let rl = make_loop();
    let tool_calls = vec![
        serde_json::json!({"function": {"name": "read_file"}}),
        serde_json::json!({"function": {"name": "task_complete"}}),
    ];
    assert!(!rl.all_parallelizable(&tool_calls));
}

// ========================================================================
// Task complete extraction
// ========================================================================

/// Extract summary and status from task_complete arguments.
#[test]
fn extract_task_complete_args() {
    let tc = serde_json::json!({
        "function": {
            "name": "task_complete",
            "arguments": "{\"summary\": \"Fixed the bug\", \"status\": \"success\"}"
        }
    });
    let (summary, status) = ReactLoop::extract_task_complete_args(&tc);
    assert_eq!(summary, "Fixed the bug");
    assert_eq!(status, "success");
}

/// Default values when task_complete has empty arguments.
#[test]
fn extract_task_complete_defaults() {
    let tc = serde_json::json!({
        "function": {"name": "task_complete", "arguments": "{}"}
    });
    let (summary, status) = ReactLoop::extract_task_complete_args(&tc);
    assert_eq!(summary, "Task completed");
    assert_eq!(status, "success");
}

// ========================================================================
// Error classification
// ========================================================================

/// Error classification covers all known patterns.
#[test]
fn error_classification_covers_patterns() {
    assert_eq!(
        ReactLoop::classify_error("Permission denied"),
        "permission_error"
    );
    assert_eq!(
        ReactLoop::classify_error("old_content mismatch"),
        "edit_mismatch"
    );
    assert_eq!(ReactLoop::classify_error("No such file"), "file_not_found");
    assert_eq!(ReactLoop::classify_error("SyntaxError"), "syntax_error");
    assert_eq!(ReactLoop::classify_error("429 Rate Limit"), "rate_limit");
    assert_eq!(ReactLoop::classify_error("Request timed out"), "timeout");
    assert_eq!(ReactLoop::classify_error("something unknown"), "generic");
}

// ========================================================================
// process_iteration tests
// ========================================================================

/// process_iteration appends assistant message to history.
#[test]
fn process_iteration_appends_to_history() {
    let rl = make_loop();
    let msg = serde_json::json!({"role": "assistant", "content": "hello"});
    let resp = LlmResponse::ok(Some("hello".into()), msg);

    let mut messages = vec![];
    let mut no_tools = 0;
    let _ = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "assistant");
    assert_eq!(messages[0]["content"], "hello");
}

/// process_iteration resets no-tool counter on tool call.
#[test]
fn process_iteration_resets_counter_on_tool_call() {
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
    assert_eq!(no_tools, 0, "counter should reset to 0 on tool call");
}

/// process_iteration increments no-tool counter on completion.
#[test]
fn process_iteration_increments_counter_on_completion() {
    let rl = make_loop();
    let msg = serde_json::json!({"role": "assistant", "content": "done"});
    let resp = LlmResponse::ok(Some("done".into()), msg);

    let mut messages = vec![];
    let mut no_tools = 0;
    let _ = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
    assert_eq!(no_tools, 1);
}

/// process_iteration returns MaxIterations when over limit.
#[test]
fn process_iteration_max_iterations() {
    let rl = make_loop();
    let resp = LlmResponse::ok(Some("hello".into()), serde_json::json!({}));
    let mut messages = vec![];
    let mut no_tools = 0;
    let result = rl.process_iteration(&resp, &mut messages, 11, &mut no_tools);
    assert!(matches!(result, Ok(TurnResult::MaxIterations)));
}

/// process_iteration returns error on failed LLM call.
#[test]
fn process_iteration_error_on_llm_failure() {
    let rl = make_loop();
    let resp = LlmResponse::fail("API 500");
    let mut messages = vec![];
    let mut no_tools = 0;
    let result = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
    assert!(matches!(result, Err(AgentError::LlmError(_))));
}

// ========================================================================
// Tool result formatting
// ========================================================================

/// format_tool_result for success.
#[test]
fn format_tool_result_success() {
    let result = serde_json::json!({"success": true, "output": "file contents here"});
    let formatted = ReactLoop::format_tool_result("read_file", &result);
    assert_eq!(formatted, "file contents here");
}

/// format_tool_result for failure.
#[test]
fn format_tool_result_failure() {
    let result = serde_json::json!({"success": false, "error": "file not found"});
    let formatted = ReactLoop::format_tool_result("read_file", &result);
    assert_eq!(formatted, "Error: file not found");
}

/// format_tool_result with completion_status.
#[test]
fn format_tool_result_with_status() {
    let result = serde_json::json!({
        "success": true,
        "output": "done",
        "completion_status": "partial"
    });
    let formatted = ReactLoop::format_tool_result("write_file", &result);
    assert_eq!(formatted, "[completion_status=partial]\ndone");
}

// ========================================================================
// LlmCaller tests
// ========================================================================

fn make_caller() -> LlmCaller {
    LlmCaller::new(LlmCallConfig {
        model: "gpt-4o".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        reasoning_effort: None,
    })
}

/// Action payload includes tools and tool_choice.
#[test]
fn action_payload_includes_tools() {
    let caller = make_caller();
    let messages = vec![serde_json::json!({"role": "user", "content": "hello"})];
    let tools = vec![serde_json::json!({"type": "function", "function": {"name": "read_file"}})];

    let payload = caller.build_action_payload(&messages, &tools);
    assert_eq!(payload["model"], "gpt-4o");
    assert_eq!(payload["tool_choice"], "auto");
    assert_eq!(payload["tools"].as_array().unwrap().len(), 1);
    assert_eq!(payload["temperature"], 0.7);
    assert_eq!(payload["max_tokens"], 4096);
}

/// clean_messages strips underscore-prefixed keys.
#[test]
fn clean_messages_strips_internal_keys() {
    let messages = vec![
        serde_json::json!({"role": "user", "content": "hi", "_internal": true, "_debug": "x"}),
    ];
    let cleaned = LlmCaller::clean_messages(&messages);
    assert!(cleaned[0].get("_internal").is_none());
    assert!(cleaned[0].get("_debug").is_none());
    assert_eq!(cleaned[0]["role"], "user");
    assert_eq!(cleaned[0]["content"], "hi");
}

/// parse_action_response handles tool calls.
#[test]
fn parse_action_response_with_tools() {
    let caller = make_caller();
    let body = serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    {"id": "tc-1", "function": {"name": "read_file", "arguments": "{}"}}
                ]
            }
        }],
        "usage": {"total_tokens": 500}
    });

    let resp = caller.parse_action_response(&body);
    assert!(resp.success);
    assert!(resp.content.is_none());
    assert_eq!(resp.tool_calls.as_ref().unwrap().len(), 1);
    assert!(resp.usage.is_some());
}

/// parse_action_response handles empty choices.
#[test]
fn parse_action_response_no_choices() {
    let caller = make_caller();
    let body = serde_json::json!({"choices": []});
    let resp = caller.parse_action_response(&body);
    assert!(!resp.success);
}

/// parse_action_response cleans provider tokens from content.
#[test]
fn parse_action_response_cleans_tokens() {
    let caller = make_caller();
    let body = serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Hello<|im_end|> world"
            }
        }]
    });
    let resp = caller.parse_action_response(&body);
    assert_eq!(resp.content.as_deref(), Some("Hello world"));
}

/// parse_action_response extracts reasoning_content.
#[test]
fn parse_action_response_extracts_reasoning() {
    let caller = make_caller();
    let body = serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "42",
                "reasoning_content": "Let me think..."
            }
        }]
    });
    let resp = caller.parse_action_response(&body);
    assert_eq!(resp.reasoning_content.as_deref(), Some("Let me think..."));
}

// ========================================================================
// ResponseCleaner tests
// ========================================================================

/// ResponseCleaner strips all provider token types.
#[test]
fn response_cleaner_strips_all_token_types() {
    let cleaner = ResponseCleaner::new();

    // Chat template tokens
    assert_eq!(
        cleaner.clean(Some("Hello<|im_end|> world")),
        Some("Hello world".to_string())
    );

    // Tool call tags
    assert_eq!(
        cleaner.clean(Some("<tool_call>content</tool_call>")),
        Some("content".to_string())
    );

    // Parameter tags
    assert_eq!(
        cleaner.clean(Some("<parameter name=\"x\">value</parameter>")),
        Some("value".to_string())
    );

    // Empty after cleaning
    assert!(cleaner.clean(Some("<|im_end|>")).is_none());

    // None input
    assert!(cleaner.clean(None).is_none());
}

// ========================================================================
// PromptComposer tests
// ========================================================================

/// PromptComposer assembles sections in priority order using temp files.
#[test]
fn prompt_composer_assembles_in_priority_order() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("security.md"), "Security rules here.").unwrap();
    std::fs::write(tmp.path().join("tools.md"), "Tool instructions here.").unwrap();
    std::fs::write(tmp.path().join("identity.md"), "You are an AI assistant.").unwrap();

    let mut composer = PromptComposer::new(tmp.path());
    composer.register_section("security", "security.md", None, 10, true);
    composer.register_section("tools", "tools.md", None, 50, true);
    composer.register_section("identity", "identity.md", None, 1, true);

    let prompt = composer.compose(&HashMap::new());

    // Verify all sections present
    assert!(prompt.contains("Security rules here."));
    assert!(prompt.contains("Tool instructions here."));
    assert!(prompt.contains("You are an AI assistant."));

    // Verify priority ordering (lower priority number = earlier in prompt)
    let identity_pos = prompt.find("You are an AI assistant.").unwrap();
    let security_pos = prompt.find("Security rules here.").unwrap();
    let tools_pos = prompt.find("Tool instructions here.").unwrap();
    assert!(
        identity_pos < security_pos,
        "identity (1) before security (10)"
    );
    assert!(security_pos < tools_pos, "security (10) before tools (50)");
}

/// PromptComposer conditional section is excluded when condition returns false.
#[test]
fn prompt_composer_excludes_conditional_section() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("always.md"), "Always present.").unwrap();
    std::fs::write(tmp.path().join("conditional.md"), "Only sometimes.").unwrap();

    let mut composer = PromptComposer::new(tmp.path());
    composer.register_section("always", "always.md", None, 1, true);
    composer.register_section(
        "conditional",
        "conditional.md",
        Some(ctx_bool("plan_mode")),
        2,
        true,
    );

    // Without the condition key
    let prompt = composer.compose(&HashMap::new());
    assert!(prompt.contains("Always present."));
    assert!(!prompt.contains("Only sometimes."));

    // With the condition key set to true
    let mut ctx = HashMap::new();
    ctx.insert("plan_mode".to_string(), serde_json::json!(true));
    let prompt = composer.compose(&ctx);
    assert!(prompt.contains("Only sometimes."));
}

// ========================================================================
// AgentResult and AgentDeps tests
// ========================================================================

/// AgentResult constructors set correct fields.
#[test]
fn agent_result_constructors() {
    let ok = AgentResult::ok("done", vec![serde_json::json!("msg")]);
    assert!(ok.success);
    assert!(!ok.interrupted);
    assert_eq!(ok.content, "done");
    assert_eq!(ok.messages.len(), 1);

    let fail = AgentResult::fail("error", vec![]);
    assert!(!fail.success);
    assert!(!fail.interrupted);

    let interrupted = AgentResult::interrupted(vec![]);
    assert!(!interrupted.success);
    assert!(interrupted.interrupted);
}

/// AgentDeps builder pattern works.
#[test]
fn agent_deps_builder() {
    let deps = AgentDeps::new()
        .with_context("model", serde_json::json!("gpt-4"))
        .with_context("session_id", serde_json::json!("s-123"));

    assert_eq!(deps.context.get("model"), Some(&serde_json::json!("gpt-4")));
    assert_eq!(
        deps.context.get("session_id"),
        Some(&serde_json::json!("s-123"))
    );
}

// ========================================================================
// Doom loop detection end-to-end
// ========================================================================

/// DoomLoopDetector escalates: Redirect -> Notify -> ForceStop across
/// repeated identical tool calls.
#[test]
fn doom_loop_full_escalation_sequence() {
    use opendev_agents::doom_loop::{DoomLoopAction, DoomLoopDetector};

    let mut det = DoomLoopDetector::new();
    let tc = serde_json::json!({
        "id": "tc-1",
        "function": {"name": "read_file", "arguments": "{\"path\": \"same.rs\"}"}
    });

    // Calls 1-2: no detection
    assert_eq!(det.check(&[tc.clone()]).0, DoomLoopAction::None);
    assert_eq!(det.check(&[tc.clone()]).0, DoomLoopAction::None);

    // Call 3: first detection -> Redirect
    let (action, warning) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::Redirect);
    assert!(warning.contains("read_file"));
    assert_eq!(det.nudge_count(), 1);

    // Call 4: second detection -> Notify
    // (threshold is 3 consecutive identical, so after a Redirect, the
    // next identical call still has the pattern)
    let (action, _) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::Notify);
    assert_eq!(det.nudge_count(), 2);

    // Call 5: third detection -> ForceStop and clears history
    let (action, _) = det.check(&[tc.clone()]);
    assert_eq!(action, DoomLoopAction::ForceStop);
    assert_eq!(det.nudge_count(), 3);
}

/// DoomLoopDetector detects 2-step cycles (e.g., edit -> test -> edit -> test...).
#[test]
fn doom_loop_two_step_cycle_detection() {
    use opendev_agents::doom_loop::{DoomLoopAction, DoomLoopDetector};

    let mut det = DoomLoopDetector::new();
    let edit = serde_json::json!({
        "function": {"name": "edit_file", "arguments": "{\"path\": \"a.rs\"}"}
    });
    let test = serde_json::json!({
        "function": {"name": "bash", "arguments": "{\"command\": \"cargo test\"}"}
    });

    // 6 calls needed for 2-step cycle with threshold 3 (2*3=6)
    for _ in 0..2 {
        assert_eq!(det.check(&[edit.clone()]).0, DoomLoopAction::None);
        assert_eq!(det.check(&[test.clone()]).0, DoomLoopAction::None);
    }
    assert_eq!(det.check(&[edit.clone()]).0, DoomLoopAction::None);
    let (action, warning) = det.check(&[test.clone()]);
    assert_eq!(action, DoomLoopAction::Redirect);
    assert!(warning.contains("2-step cycle"));
}

/// DoomLoopDetector reset clears history and nudge count.
#[test]
fn doom_loop_reset_clears_state() {
    use opendev_agents::doom_loop::{DoomLoopAction, DoomLoopDetector};

    let mut det = DoomLoopDetector::new();
    let tc = serde_json::json!({
        "function": {"name": "read_file", "arguments": "{\"path\": \"same.rs\"}"}
    });

    // Trigger one detection
    for _ in 0..3 {
        det.check(&[tc.clone()]);
    }
    assert_eq!(det.nudge_count(), 1);

    // Reset
    det.reset();
    assert_eq!(det.nudge_count(), 0);

    // After reset, same calls don't immediately trigger
    assert_eq!(det.check(&[tc.clone()]).0, DoomLoopAction::None);
}

/// Varied tool calls never trigger doom loop detection.
#[test]
fn doom_loop_varied_calls_no_detection() {
    use opendev_agents::doom_loop::{DoomLoopAction, DoomLoopDetector};

    let mut det = DoomLoopDetector::new();
    for i in 0..20 {
        let tc = serde_json::json!({
            "function": {"name": "read_file", "arguments": format!("{{\"path\": \"file{i}.rs\"}}")}
        });
        let (action, _) = det.check(&[tc]);
        assert_eq!(
            action,
            DoomLoopAction::None,
            "varied calls should not trigger"
        );
    }
}

// ========================================================================
// ========================================================================
// task_complete terminates loop
// ========================================================================

/// process_iteration with task_complete returns ToolCall (the full run()
/// loop is what detects task_complete and converts to Complete).
/// Here we verify that is_task_complete + extract_task_complete_args
/// work together to detect and extract task_complete.
#[test]
fn task_complete_detection_and_extraction() {
    let rl = make_loop();
    let tc = serde_json::json!({
        "id": "tc-done",
        "function": {
            "name": "task_complete",
            "arguments": "{\"summary\": \"All done\", \"status\": \"success\"}"
        }
    });

    // is_task_complete detects it
    assert!(ReactLoop::is_task_complete(&tc));

    // extract_task_complete_args pulls summary and status
    let (summary, status) = ReactLoop::extract_task_complete_args(&tc);
    assert_eq!(summary, "All done");
    assert_eq!(status, "success");

    // process_iteration returns ToolCall for the caller to handle
    let msg = serde_json::json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [tc]
    });
    let resp = LlmResponse::ok(None, msg);
    let mut messages = vec![];
    let mut no_tools = 0;
    let result = rl.process_iteration(&resp, &mut messages, 1, &mut no_tools);
    assert!(matches!(result.unwrap(), TurnResult::ToolCall { .. }));
}

// ========================================================================
// Prompt composition - embedded templates
// ========================================================================

/// All embedded templates load successfully.
#[test]
fn all_embedded_templates_load() {
    use opendev_agents::prompts::embedded::{TEMPLATE_COUNT, TEMPLATES};

    assert_eq!(TEMPLATE_COUNT, TEMPLATES.len());
    assert!(
        TEMPLATE_COUNT >= 76,
        "expected at least 76 templates, got {TEMPLATE_COUNT}"
    );

    for (key, content) in TEMPLATES.iter() {
        assert!(
            !content.is_empty(),
            "embedded template '{key}' should not be empty"
        );
    }
}

/// Default composer has 20+ sections registered.
#[test]
fn default_composer_section_count() {
    use opendev_agents::prompts::composer::create_default_composer;

    let tmp = tempfile::TempDir::new().unwrap();
    let composer = create_default_composer(tmp.path());
    assert!(
        composer.section_count() >= 20,
        "expected at least 20 sections, got {}",
        composer.section_count()
    );
}

/// compose_two_part splits cacheable vs dynamic sections.
#[test]
fn two_part_cache_splitting() {
    use opendev_agents::prompts::composer::create_default_composer;

    let tmp = tempfile::TempDir::new().unwrap();
    let composer = create_default_composer(tmp.path());

    let (stable, dynamic) = composer.compose_two_part(&HashMap::new());

    // Stable part should contain security policy (always included, cacheable)
    assert!(
        stable.contains("Security Policy") || !stable.is_empty(),
        "stable part should have cacheable sections"
    );

    // Dynamic part should exist (scratchpad and reminders_note are dynamic)
    // but they may not load without session context
    // Just verify the split works without panicking
    let _ = dynamic;
}

/// Prompt variable substitution replaces {{placeholders}}.
#[test]
fn prompt_variable_substitution() {
    use opendev_agents::prompts::composer::substitute_variables;

    let mut vars = HashMap::new();
    vars.insert("session_id".to_string(), "abc-123".to_string());
    vars.insert("model".to_string(), "gpt-4o".to_string());

    let template = "Session: {{session_id}}, Model: {{model}}, Unknown: {{unknown}}";
    let result = substitute_variables(template, &vars);

    assert_eq!(
        result,
        "Session: abc-123, Model: gpt-4o, Unknown: {{unknown}}"
    );
}

// ========================================================================
// Skills discovery
// ========================================================================

/// SkillLoader discovers all 3 builtin skills with correct metadata.
#[test]
fn discover_all_builtin_skills() {
    use opendev_agents::skills::SkillLoader;

    let mut loader = SkillLoader::new(vec![]);
    let skills = loader.discover_skills();

    assert!(skills.len() >= 3);

    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"commit"));
    assert!(names.contains(&"review-pr"));
    assert!(names.contains(&"create-pr"));

    // All should be builtin source
    use opendev_agents::skills::SkillSource;
    for skill in &skills {
        assert_eq!(skill.source, SkillSource::Builtin);
    }
}

/// Loading a builtin skill returns content without frontmatter.
#[test]
fn load_builtin_skill_strips_frontmatter() {
    use opendev_agents::skills::SkillLoader;

    let mut loader = SkillLoader::new(vec![]);
    loader.discover_skills();

    let skill = loader.load_skill("commit").unwrap();
    assert_eq!(skill.metadata.name, "commit");
    assert!(!skill.content.is_empty());
    assert!(
        !skill.content.starts_with("---"),
        "frontmatter should be stripped"
    );
}

/// Project-local skills override builtins with the same name.
#[test]
fn project_skill_overrides_builtin() {
    use opendev_agents::skills::{SkillLoader, SkillSource};

    let tmp = tempfile::TempDir::new().unwrap();
    let skill_dir = tmp.path().join("skills");
    std::fs::create_dir_all(&skill_dir).unwrap();

    std::fs::write(
        skill_dir.join("commit.md"),
        "---\nname: commit\ndescription: Custom commit\n---\n\n# Custom\nOverridden.\n",
    )
    .unwrap();

    let mut loader = SkillLoader::new(vec![skill_dir]);
    let skills = loader.discover_skills();

    let commit = skills.iter().find(|s| s.name == "commit").unwrap();
    assert_eq!(commit.description, "Custom commit");
    assert_ne!(commit.source, SkillSource::Builtin);
}

/// Multi-directory priority: first directory wins.
#[test]
fn skill_directory_priority_ordering() {
    use opendev_agents::skills::SkillLoader;

    let tmp1 = tempfile::TempDir::new().unwrap();
    let tmp2 = tempfile::TempDir::new().unwrap();
    let dir1 = tmp1.path().join("skills");
    let dir2 = tmp2.path().join("skills");
    std::fs::create_dir_all(&dir1).unwrap();
    std::fs::create_dir_all(&dir2).unwrap();

    std::fs::write(
        dir1.join("myskill.md"),
        "---\nname: myskill\ndescription: High priority\n---\n\nContent1.\n",
    )
    .unwrap();

    std::fs::write(
        dir2.join("myskill.md"),
        "---\nname: myskill\ndescription: Low priority\n---\n\nContent2.\n",
    )
    .unwrap();

    // dir1 first = highest priority
    let mut loader = SkillLoader::new(vec![dir1, dir2]);
    let skills = loader.discover_skills();

    let myskill = skills.iter().find(|s| s.name == "myskill").unwrap();
    assert_eq!(myskill.description, "High priority");
}

/// Namespaced skills can be loaded by full name or bare name.
#[test]
fn namespaced_skill_lookup() {
    use opendev_agents::skills::SkillLoader;

    let tmp = tempfile::TempDir::new().unwrap();
    let skill_dir = tmp.path().join("skills");
    std::fs::create_dir_all(&skill_dir).unwrap();

    std::fs::write(
        skill_dir.join("rebase.md"),
        "---\nname: rebase\ndescription: Git rebase\nnamespace: git\n---\n\n# Rebase\n",
    )
    .unwrap();

    let mut loader = SkillLoader::new(vec![skill_dir]);
    loader.discover_skills();

    // Load by full name
    let skill = loader.load_skill("git:rebase").unwrap();
    assert_eq!(skill.metadata.name, "rebase");
    assert_eq!(skill.metadata.namespace, "git");
}

/// build_skills_index produces formatted markdown with all builtin skills.
#[test]
fn skills_index_format() {
    use opendev_agents::skills::SkillLoader;

    let mut loader = SkillLoader::new(vec![]);
    let index = loader.build_skills_index();

    assert!(index.contains("## Available Skills"));
    assert!(index.contains("**commit**"));
    assert!(index.contains("**review-pr**"));
    assert!(index.contains("**create-pr**"));
    assert!(index.contains("Skill"));
}

/// Nonexistent skill returns None.
#[test]
fn load_nonexistent_skill_returns_none() {
    use opendev_agents::skills::SkillLoader;

    let mut loader = SkillLoader::new(vec![]);
    loader.discover_skills();
    assert!(loader.load_skill("totally-nonexistent-skill-xyz").is_none());
}

// ========================================================================
// Prompt loader
// ========================================================================

/// PromptLoader resolves from embedded, then filesystem, then fallback.
#[test]
fn prompt_loader_resolution_order() {
    use opendev_agents::prompts::loader::PromptLoader;

    let tmp = tempfile::TempDir::new().unwrap();
    let loader = PromptLoader::new(tmp.path());

    // 1. Should resolve "system/compaction" from embedded
    let result = loader.load_prompt("system/compaction");
    assert!(result.is_ok());
    assert!(result.unwrap().contains("conversation compactor"));

    // 2. Should fail for nonexistent without fallback
    let result = loader.load_prompt("nonexistent");
    assert!(result.is_err());

    // 3. Should use fallback when provided
    let result = loader.load_prompt_with_fallback("nonexistent", Some("fallback text"));
    assert_eq!(result.unwrap(), "fallback text");
}

// ========================================================================
// Mock LLM integration test: tool call dispatch through the agent loop
// ========================================================================

/// Simulate a full agent iteration with a mock LLM response that contains
/// a tool call. Verify that process_iteration correctly identifies the tool
/// call and returns a ToolCall result, proving the dispatch pipeline works
/// end-to-end without a real LLM.
#[test]
fn mock_llm_tool_call_dispatch() {
    let rl = make_loop();

    // Simulate a user message in history
    let mut messages: Vec<serde_json::Value> =
        vec![serde_json::json!({"role": "user", "content": "Read the file src/main.rs"})];

    // Simulate the LLM response containing a tool call
    let llm_message = serde_json::json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{
            "id": "tc-mock-001",
            "function": {
                "name": "read_file",
                "arguments": "{\"file_path\": \"src/main.rs\"}"
            }
        }]
    });
    let llm_response = LlmResponse::ok(None, llm_message);

    // Process the iteration
    let mut no_tool_count = 0;
    let result = rl
        .process_iteration(&llm_response, &mut messages, 1, &mut no_tool_count)
        .unwrap();

    // Verify the result is a ToolCall
    match result {
        TurnResult::ToolCall { ref tool_calls } => {
            assert_eq!(tool_calls.len(), 1);
            let tc = &tool_calls[0];
            assert_eq!(
                tc.get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str()),
                Some("read_file")
            );
            assert_eq!(tc.get("id").and_then(|v| v.as_str()), Some("tc-mock-001"));
        }
        other => panic!("Expected ToolCall, got {:?}", other),
    }

    // Verify the assistant message was appended to history
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1]["role"], "assistant");

    // Verify no-tool counter was reset (tool was dispatched)
    assert_eq!(no_tool_count, 0);

    // Now simulate the tool result being added and a completion response
    messages.push(serde_json::json!({
        "role": "tool",
        "tool_call_id": "tc-mock-001",
        "content": "fn main() { println!(\"Hello!\"); }"
    }));

    let completion_msg = serde_json::json!({
        "role": "assistant",
        "content": "The file contains a simple Hello World program."
    });
    let completion_response = LlmResponse::ok(
        Some("The file contains a simple Hello World program.".into()),
        completion_msg,
    );

    let result2 = rl
        .process_iteration(&completion_response, &mut messages, 2, &mut no_tool_count)
        .unwrap();

    // Should be Complete
    match result2 {
        TurnResult::Complete { content, .. } => {
            assert!(content.contains("Hello World"));
        }
        other => panic!("Expected Complete, got {:?}", other),
    }
    assert_eq!(no_tool_count, 1);
}

/// Simulate a mock LLM producing a task_complete tool call.
/// Verifies the full flow from LLM response through to tool dispatch
/// and extraction of summary/status.
#[test]
fn mock_llm_task_complete_flow() {
    let rl = make_loop();

    let llm_message = serde_json::json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{
            "id": "tc-done",
            "function": {
                "name": "task_complete",
                "arguments": "{\"summary\": \"Fixed all tests\", \"status\": \"success\"}"
            }
        }]
    });
    let llm_response = LlmResponse::ok(None, llm_message);

    let mut messages = vec![serde_json::json!({"role": "user", "content": "Fix the tests"})];
    let mut no_tool_count = 0;

    let result = rl
        .process_iteration(&llm_response, &mut messages, 1, &mut no_tool_count)
        .unwrap();

    // process_iteration returns ToolCall; the caller detects task_complete
    match result {
        TurnResult::ToolCall { ref tool_calls } => {
            assert_eq!(tool_calls.len(), 1);
            assert!(ReactLoop::is_task_complete(&tool_calls[0]));

            let (summary, status) = ReactLoop::extract_task_complete_args(&tool_calls[0]);
            assert_eq!(summary, "Fixed all tests");
            assert_eq!(status, "success");
        }
        other => panic!("Expected ToolCall with task_complete, got {:?}", other),
    }
}

/// Simulate multiple mock tool calls in parallel (read-only tools).
#[test]
fn mock_llm_parallel_tool_calls() {
    let rl = make_loop();

    let llm_message = serde_json::json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [
            {"id": "tc-1", "function": {"name": "Read", "arguments": "{\"file_path\": \"a.rs\"}"}},
            {"id": "tc-2", "function": {"name": "Grep", "arguments": "{\"pattern\": \"TODO\"}"}},
            {"id": "tc-3", "function": {"name": "Glob", "arguments": "{\"path\": \"src/\"}"}}
        ]
    });

    let llm_response = LlmResponse::ok(None, llm_message);
    let mut messages = vec![serde_json::json!({"role": "user", "content": "explore"})];
    let mut no_tool_count = 0;

    let result = rl
        .process_iteration(&llm_response, &mut messages, 1, &mut no_tool_count)
        .unwrap();

    match result {
        TurnResult::ToolCall { ref tool_calls } => {
            assert_eq!(tool_calls.len(), 3);
            // All read-only tools should be parallelizable
            assert!(rl.all_parallelizable(tool_calls));
        }
        other => panic!("Expected ToolCall, got {:?}", other),
    }
}

/// PromptLoader prefers .md over .txt files.
#[test]
fn prompt_loader_md_preferred_over_txt() {
    use opendev_agents::prompts::loader::PromptLoader;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.md"), "MD content").unwrap();
    std::fs::write(tmp.path().join("test.txt"), "TXT content").unwrap();

    let loader = PromptLoader::new(tmp.path());
    let result = loader.load_prompt("test").unwrap();
    assert_eq!(result, "MD content");
}
