use super::*;

#[test]
fn test_agent_result_ok() {
    let result = AgentResult::ok("done", vec![]);
    assert!(result.success);
    assert!(!result.interrupted);
    assert!(!result.backgrounded);
    assert_eq!(result.content, "done");
}

#[test]
fn test_agent_result_fail() {
    let result = AgentResult::fail("error", vec![]);
    assert!(!result.success);
    assert!(!result.interrupted);
    assert!(!result.backgrounded);
    assert_eq!(result.content, "error");
}

#[test]
fn test_agent_result_interrupted() {
    let result = AgentResult::interrupted(vec![]);
    assert!(!result.success);
    assert!(result.interrupted);
    assert!(!result.backgrounded);
}

#[test]
fn test_agent_result_backgrounded() {
    let result = AgentResult::backgrounded(vec![]);
    assert!(!result.success);
    assert!(!result.interrupted);
    assert!(result.backgrounded);
}

#[test]
fn test_llm_response_ok() {
    let msg = serde_json::json!({"role": "assistant", "content": "hello"});
    let resp = LlmResponse::ok(Some("hello".into()), msg);
    assert!(resp.success);
    assert_eq!(resp.content.as_deref(), Some("hello"));
    assert!(!resp.interrupted);
}

#[test]
fn test_llm_response_fail() {
    let resp = LlmResponse::fail("API error 500");
    assert!(!resp.success);
    assert_eq!(resp.error.as_deref(), Some("API error 500"));
}

#[test]
fn test_llm_response_interrupted() {
    let resp = LlmResponse::interrupted();
    assert!(!resp.success);
    assert!(resp.interrupted);
}

#[test]
fn test_agent_deps_builder() {
    let deps = AgentDeps::new().with_context("key", serde_json::json!("value"));
    assert_eq!(deps.context.get("key"), Some(&serde_json::json!("value")));
}

#[test]
fn test_task_monitor_background_default() {
    struct DummyMonitor;
    impl TaskMonitor for DummyMonitor {
        fn should_interrupt(&self) -> bool {
            false
        }
    }
    let m = DummyMonitor;
    // Default implementation returns false
    assert!(!m.is_background_requested());
}

#[test]
fn test_interrupt_token_as_task_monitor_background() {
    let token = InterruptToken::new();
    let monitor: &dyn TaskMonitor = &token;
    assert!(!monitor.is_background_requested());
    token.request_background();
    assert!(monitor.is_background_requested());
    // Background should NOT trigger should_interrupt
    assert!(!monitor.should_interrupt());
}

#[test]
fn test_llm_response_constructors_finish_reason_none() {
    let msg = serde_json::json!({"role": "assistant", "content": "hi"});
    let ok = LlmResponse::ok(Some("hi".into()), msg);
    assert!(ok.finish_reason.is_none());

    let fail = LlmResponse::fail("err");
    assert!(fail.finish_reason.is_none());

    let interrupted = LlmResponse::interrupted();
    assert!(interrupted.finish_reason.is_none());
}

#[test]
fn test_agent_error_display() {
    let err = AgentError::MaxIterations(10);
    assert_eq!(err.to_string(), "Max iterations reached (10)");

    let err = AgentError::ApiError {
        status: 429,
        message: "rate limited".into(),
    };
    assert_eq!(err.to_string(), "API error 429: rate limited");
}
