use super::*;

#[test]
fn test_new_is_empty() {
    let ctrl = AgentCreatorController::new();
    assert_eq!(ctrl.name(), "");
    assert_eq!(ctrl.description(), "");
    assert_eq!(ctrl.model(), None);
    assert!(ctrl.tools().is_empty());
    assert_eq!(ctrl.instructions(), "");
}

#[test]
fn test_set_fields() {
    let mut ctrl = AgentCreatorController::new();
    ctrl.set_name("my-agent");
    ctrl.set_description("A helpful agent");
    ctrl.set_model(Some("gpt-4".into()));
    ctrl.set_instructions("You are a coding assistant.\nBe concise.");

    assert_eq!(ctrl.name(), "my-agent");
    assert_eq!(ctrl.description(), "A helpful agent");
    assert_eq!(ctrl.model(), Some("gpt-4"));
    assert_eq!(
        ctrl.instructions(),
        "You are a coding assistant.\nBe concise."
    );
}

#[test]
fn test_add_and_remove_tools() {
    let mut ctrl = AgentCreatorController::new();
    ctrl.add_tool("bash");
    ctrl.add_tool("file_read");
    ctrl.add_tool("bash"); // duplicate
    assert_eq!(ctrl.tools(), &["bash", "file_read"]);

    assert!(ctrl.remove_tool("bash"));
    assert_eq!(ctrl.tools(), &["file_read"]);

    assert!(!ctrl.remove_tool("nonexistent"));
}

#[test]
fn test_validate_success() {
    let mut ctrl = AgentCreatorController::new();
    ctrl.set_name("test-agent");
    ctrl.set_description("desc");
    ctrl.set_instructions("do stuff");
    ctrl.add_tool("bash");

    let spec = ctrl.validate().unwrap();
    assert_eq!(spec.name, "test-agent");
    assert_eq!(spec.description, "desc");
    assert_eq!(spec.model, None);
    assert_eq!(spec.tools, vec!["bash".to_string()]);
    assert_eq!(spec.instructions, "do stuff");
}

#[test]
fn test_validate_with_model() {
    let mut ctrl = AgentCreatorController::new();
    ctrl.set_name("agent");
    ctrl.set_description("desc");
    ctrl.set_model(Some("claude-3".into()));
    ctrl.set_instructions("instructions");

    let spec = ctrl.validate().unwrap();
    assert_eq!(spec.model, Some("claude-3".into()));
}

#[test]
fn test_validate_missing_name() {
    let ctrl = AgentCreatorController::new();
    let err = ctrl.validate().unwrap_err();
    assert!(err.contains("name"), "Error should mention name: {err}");
}

#[test]
fn test_validate_missing_description() {
    let mut ctrl = AgentCreatorController::new();
    ctrl.set_name("agent");
    let err = ctrl.validate().unwrap_err();
    assert!(
        err.contains("description"),
        "Error should mention description: {err}"
    );
}

#[test]
fn test_validate_missing_instructions() {
    let mut ctrl = AgentCreatorController::new();
    ctrl.set_name("agent");
    ctrl.set_description("desc");
    let err = ctrl.validate().unwrap_err();
    assert!(
        err.contains("instructions"),
        "Error should mention instructions: {err}"
    );
}

#[test]
fn test_validate_trims_whitespace() {
    let mut ctrl = AgentCreatorController::new();
    ctrl.set_name("  agent  ");
    ctrl.set_description("  desc  ");
    ctrl.set_instructions("instructions");

    let spec = ctrl.validate().unwrap();
    assert_eq!(spec.name, "agent");
    assert_eq!(spec.description, "desc");
}

#[test]
fn test_validate_whitespace_only_is_invalid() {
    let mut ctrl = AgentCreatorController::new();
    ctrl.set_name("   ");
    assert!(ctrl.validate().is_err());
}

#[test]
fn test_reset() {
    let mut ctrl = AgentCreatorController::new();
    ctrl.set_name("agent");
    ctrl.set_description("desc");
    ctrl.set_model(Some("model".into()));
    ctrl.add_tool("bash");
    ctrl.set_instructions("instr");

    ctrl.reset();

    assert_eq!(ctrl.name(), "");
    assert_eq!(ctrl.description(), "");
    assert_eq!(ctrl.model(), None);
    assert!(ctrl.tools().is_empty());
    assert_eq!(ctrl.instructions(), "");
}

#[test]
fn test_validate_empty_tools_is_ok() {
    let mut ctrl = AgentCreatorController::new();
    ctrl.set_name("agent");
    ctrl.set_description("desc");
    ctrl.set_instructions("instr");

    let spec = ctrl.validate().unwrap();
    assert!(spec.tools.is_empty());
}

#[test]
fn test_default_trait() {
    let ctrl = AgentCreatorController::default();
    assert_eq!(ctrl.name(), "");
}
