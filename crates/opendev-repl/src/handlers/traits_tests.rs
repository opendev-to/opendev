use super::*;

struct TestHandler;

impl ToolHandler for TestHandler {
    fn handles(&self) -> &[&str] {
        &["test_tool"]
    }
}

#[test]
fn test_default_pre_check_allows() {
    let handler = TestHandler;
    let args = HashMap::new();
    match handler.pre_check("test_tool", &args) {
        PreCheckResult::Allow => {}
        other => panic!("Expected Allow, got {:?}", other),
    }
}

#[test]
fn test_default_post_process_passes_through() {
    let handler = TestHandler;
    let args = HashMap::new();
    let result = handler.post_process("test_tool", &args, Some("output"), None, true);
    assert!(result.success);
    assert_eq!(result.output.as_deref(), Some("output"));
    assert!(result.meta.changed_files.is_empty());
}
