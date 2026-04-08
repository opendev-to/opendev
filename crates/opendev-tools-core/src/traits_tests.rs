use super::*;

#[test]
fn test_tool_result_ok() {
    let result = ToolResult::ok("file contents here");
    assert!(result.success);
    assert_eq!(result.output.as_deref(), Some("file contents here"));
    assert!(result.error.is_none());
    assert!(result.metadata.is_empty());
}

#[test]
fn test_tool_result_ok_with_metadata() {
    let mut meta = HashMap::new();
    meta.insert("lines".into(), serde_json::json!(42));
    let result = ToolResult::ok_with_metadata("output", meta);
    assert!(result.success);
    assert_eq!(result.metadata.get("lines"), Some(&serde_json::json!(42)));
}

#[test]
fn test_tool_result_fail() {
    let result = ToolResult::fail("file not found");
    assert!(!result.success);
    assert!(result.output.is_none());
    assert_eq!(result.error.as_deref(), Some("file not found"));
}

#[test]
fn test_tool_result_from_error() {
    let err = ToolError::NotFound("read_file".into());
    let result = ToolResult::from_error(err);
    assert!(!result.success);
    assert!(result.error.as_ref().unwrap().contains("read_file"));
}

#[test]
fn test_tool_result_serde_roundtrip() {
    let result = ToolResult::ok("hello");
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: ToolResult = serde_json::from_str(&json).unwrap();
    assert!(deserialized.success);
    assert_eq!(deserialized.output.as_deref(), Some("hello"));
}

#[test]
fn test_tool_context_builder() {
    let project_dir = std::env::temp_dir().join("project");
    let ctx = ToolContext::new(&project_dir)
        .with_subagent(true)
        .with_session_id("sess-123")
        .with_value("key", serde_json::json!("value"));

    assert_eq!(ctx.working_dir, project_dir);
    assert!(ctx.is_subagent);
    assert_eq!(ctx.session_id.as_deref(), Some("sess-123"));
    assert_eq!(ctx.values.get("key"), Some(&serde_json::json!("value")));
}

#[test]
fn test_tool_context_default() {
    let ctx = ToolContext::default();
    assert!(!ctx.is_subagent);
    assert!(ctx.session_id.is_none());
}

#[test]
fn test_tool_error_display() {
    let err = ToolError::InvalidParams("missing file_path".into());
    assert_eq!(err.to_string(), "Invalid parameters: missing file_path");
}

#[test]
fn test_tool_result_duration_ms_default_none() {
    let result = ToolResult::ok("output");
    assert!(result.duration_ms.is_none());

    let result = ToolResult::fail("error");
    assert!(result.duration_ms.is_none());
}

#[test]
fn test_tool_result_duration_ms_serde() {
    let mut result = ToolResult::ok("output");
    result.duration_ms = Some(42);
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"duration_ms\":42"));
    let deserialized: ToolResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.duration_ms, Some(42));
}

#[test]
fn test_tool_result_duration_ms_skipped_when_none() {
    let result = ToolResult::ok("output");
    let json = serde_json::to_string(&result).unwrap();
    assert!(!json.contains("duration_ms"));
}

#[test]
fn test_tool_timeout_config_default() {
    let config = ToolTimeoutConfig::default();
    assert_eq!(config.idle_timeout_secs, 60);
    assert_eq!(config.max_timeout_secs, 600);
}

#[test]
fn test_tool_context_with_timeout_config() {
    let config = ToolTimeoutConfig {
        idle_timeout_secs: 30,
        max_timeout_secs: 300,
    };
    let ctx = ToolContext::new(std::env::temp_dir().join("project")).with_timeout_config(config);
    assert!(ctx.timeout_config.is_some());
    let tc = ctx.timeout_config.unwrap();
    assert_eq!(tc.idle_timeout_secs, 30);
    assert_eq!(tc.max_timeout_secs, 300);
}

#[test]
fn test_tool_context_default_no_timeout_config() {
    let ctx = ToolContext::default();
    assert!(ctx.timeout_config.is_none());
}

// --- ToolCategory tests ---

#[test]
fn test_tool_category_serde_roundtrip() {
    let cat = ToolCategory::Read;
    let json = serde_json::to_string(&cat).unwrap();
    assert_eq!(json, "\"Read\"");
    let deserialized: ToolCategory = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, ToolCategory::Read);
}

#[test]
fn test_tool_category_all_variants_serialize() {
    let variants = [
        (ToolCategory::Read, "\"Read\""),
        (ToolCategory::Write, "\"Write\""),
        (ToolCategory::Process, "\"Process\""),
        (ToolCategory::Web, "\"Web\""),
        (ToolCategory::Session, "\"Session\""),
        (ToolCategory::Memory, "\"Memory\""),
        (ToolCategory::Meta, "\"Meta\""),
        (ToolCategory::Messaging, "\"Messaging\""),
        (ToolCategory::Automation, "\"Automation\""),
        (ToolCategory::Symbol, "\"Symbol\""),
        (ToolCategory::Mcp, "\"Mcp\""),
        (ToolCategory::Other, "\"Other\""),
    ];
    for (cat, expected) in variants {
        assert_eq!(serde_json::to_string(&cat).unwrap(), expected);
    }
}

#[test]
fn test_tool_category_display() {
    assert_eq!(ToolCategory::Read.to_string(), "Read");
    assert_eq!(ToolCategory::Process.to_string(), "Process");
    assert_eq!(ToolCategory::Other.to_string(), "Other");
}

#[test]
fn test_tool_category_hash_eq() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(ToolCategory::Read);
    set.insert(ToolCategory::Read);
    set.insert(ToolCategory::Write);
    assert_eq!(set.len(), 2);
}

// --- InterruptBehavior tests ---

#[test]
fn test_interrupt_behavior_default_is_cancel() {
    assert_eq!(InterruptBehavior::default(), InterruptBehavior::Cancel);
}

#[test]
fn test_interrupt_behavior_variants() {
    assert_ne!(InterruptBehavior::Cancel, InterruptBehavior::Block);
    assert_ne!(InterruptBehavior::Block, InterruptBehavior::Ignore);
}

// --- BaseTool default method tests ---

#[derive(Debug)]
struct DefaultTestTool;

#[async_trait::async_trait]
impl BaseTool for DefaultTestTool {
    fn name(&self) -> &str {
        "test"
    }
    fn description(&self) -> &str {
        "A test tool"
    }
    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(
        &self,
        _args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        ToolResult::ok("ok")
    }
}

#[test]
fn test_default_is_read_only() {
    assert!(!DefaultTestTool.is_read_only(&HashMap::new()));
}

#[test]
fn test_default_is_destructive() {
    assert!(!DefaultTestTool.is_destructive(&HashMap::new()));
}

#[test]
fn test_default_is_concurrent_safe_delegates_to_is_read_only() {
    let args = HashMap::new();
    assert_eq!(
        DefaultTestTool.is_concurrent_safe(&args),
        DefaultTestTool.is_read_only(&args)
    );
}

#[test]
fn test_default_category() {
    assert_eq!(DefaultTestTool.category(), ToolCategory::Other);
}

#[test]
fn test_default_skip_dedup() {
    assert!(!DefaultTestTool.skip_dedup());
}

#[test]
fn test_default_is_search_or_read() {
    assert!(!DefaultTestTool.is_search_or_read(&HashMap::new()));
}

#[test]
fn test_default_is_enabled() {
    assert!(DefaultTestTool.is_enabled());
}

#[test]
fn test_default_interrupt_behavior() {
    assert_eq!(
        DefaultTestTool.interrupt_behavior(),
        InterruptBehavior::Cancel
    );
}

#[test]
fn test_default_truncation_rule() {
    assert!(DefaultTestTool.truncation_rule().is_none());
}

#[test]
fn test_default_search_hint() {
    assert!(DefaultTestTool.search_hint().is_none());
}

#[test]
fn test_default_should_defer() {
    assert!(!DefaultTestTool.should_defer());
}

#[test]
fn test_default_prompt_contribution() {
    assert!(DefaultTestTool.prompt_contribution().is_none());
}

// --- ValidationError tests ---

#[test]
fn test_validation_error_display_with_path() {
    let err = ValidationError {
        path: "file_path".to_string(),
        message: "Missing required parameter: 'file_path'".to_string(),
    };
    assert_eq!(
        err.to_string(),
        "file_path: Missing required parameter: 'file_path'"
    );
}

#[test]
fn test_validation_error_display_root_path() {
    let err = ValidationError {
        path: "root".to_string(),
        message: "Invalid object".to_string(),
    };
    assert_eq!(err.to_string(), "Invalid object");
}

#[test]
fn test_validation_error_display_empty_path() {
    let err = ValidationError {
        path: String::new(),
        message: "Something is wrong".to_string(),
    };
    assert_eq!(err.to_string(), "Something is wrong");
}

#[test]
fn test_validation_error_display_nested_path() {
    let err = ValidationError {
        path: "invocations.0.tool".to_string(),
        message: "expected type 'string', got number".to_string(),
    };
    assert_eq!(
        err.to_string(),
        "invocations.0.tool: expected type 'string', got number"
    );
}
