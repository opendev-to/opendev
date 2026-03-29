use super::*;
use crate::handlers::traits::HandlerMeta;

struct MockHandler {
    names: Vec<&'static str>,
    deny: bool,
}

impl MockHandler {
    fn allowing(names: Vec<&'static str>) -> Self {
        Self { names, deny: false }
    }

    fn denying(names: Vec<&'static str>) -> Self {
        Self { names, deny: true }
    }
}

impl ToolHandler for MockHandler {
    fn handles(&self) -> &[&str] {
        &self.names
    }

    fn pre_check(&self, _tool_name: &str, _args: &HashMap<String, Value>) -> PreCheckResult {
        if self.deny {
            PreCheckResult::Deny("denied by test".to_string())
        } else {
            PreCheckResult::Allow
        }
    }

    fn post_process(
        &self,
        _tool_name: &str,
        _args: &HashMap<String, Value>,
        output: Option<&str>,
        _error: Option<&str>,
        _success: bool,
    ) -> HandlerResult {
        HandlerResult {
            output: output.map(|s| format!("[processed] {s}")),
            error: None,
            success: true,
            meta: HandlerMeta::default(),
        }
    }
}

#[test]
fn test_empty_registry_allows_all() {
    let registry = HandlerRegistry::new();
    let args = HashMap::new();
    match registry.pre_check("anything", &args) {
        PreCheckResult::Allow => {}
        other => panic!("Expected Allow, got {:?}", other),
    }
}

#[test]
fn test_registered_handler_is_found() {
    let mut registry = HandlerRegistry::new();
    registry.register(Box::new(MockHandler::allowing(vec!["bash"])));

    let args = HashMap::new();
    match registry.pre_check("bash", &args) {
        PreCheckResult::Allow => {}
        other => panic!("Expected Allow, got {:?}", other),
    }
}

#[test]
fn test_deny_pre_check() {
    let mut registry = HandlerRegistry::new();
    registry.register(Box::new(MockHandler::denying(vec!["rm_tool"])));

    let args = HashMap::new();
    match registry.pre_check("rm_tool", &args) {
        PreCheckResult::Deny(reason) => assert!(reason.contains("denied")),
        other => panic!("Expected Deny, got {:?}", other),
    }
}

#[test]
fn test_post_process_with_handler() {
    let mut registry = HandlerRegistry::new();
    registry.register(Box::new(MockHandler::allowing(vec!["bash"])));

    let args = HashMap::new();
    let result = registry.post_process("bash", &args, Some("hello"), None, true);
    assert_eq!(result.output.as_deref(), Some("[processed] hello"));
}

#[test]
fn test_post_process_without_handler() {
    let registry = HandlerRegistry::new();
    let args = HashMap::new();
    let result = registry.post_process("unknown", &args, Some("hello"), None, true);
    assert_eq!(result.output.as_deref(), Some("hello"));
}

#[test]
fn test_unmatched_tool_passes_through() {
    let mut registry = HandlerRegistry::new();
    registry.register(Box::new(MockHandler::denying(vec!["bash"])));

    let args = HashMap::new();
    // "read_file" not handled by our handler
    match registry.pre_check("read_file", &args) {
        PreCheckResult::Allow => {}
        other => panic!("Expected Allow for unmatched tool, got {:?}", other),
    }
}
