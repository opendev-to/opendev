use super::*;

#[test]
fn test_subagent_spec_new() {
    let spec = SubAgentSpec::new("test", "A test agent", "You are a test agent.");
    assert_eq!(spec.name, "test");
    assert!(!spec.has_tool_restriction());
    assert!(spec.model.is_none());
}

#[test]
fn test_subagent_spec_serde() {
    let spec = SubAgentSpec::new("test", "desc", "prompt")
        .with_tools(vec!["read_file".into()])
        .with_model("gpt-4");

    let json = serde_json::to_string(&spec).unwrap();
    let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.name, "test");
    assert_eq!(restored.tools, vec!["read_file"]);
    assert_eq!(restored.model.as_deref(), Some("gpt-4"));
}

#[test]
fn test_subagent_spec_defaults() {
    let spec = SubAgentSpec::new("test", "desc", "prompt");
    assert!(spec.max_steps.is_none());
    assert!(!spec.hidden);
    assert!(spec.temperature.is_none());
    assert!(spec.top_p.is_none());
    assert_eq!(spec.mode, AgentMode::Subagent);
}

#[test]
fn test_subagent_spec_serde_extended_fields() {
    let spec = SubAgentSpec::new("test", "desc", "prompt")
        .with_max_steps(50)
        .with_hidden(true)
        .with_temperature(0.5);

    let json = serde_json::to_string(&spec).unwrap();
    let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.max_steps, Some(50));
    assert!(restored.hidden);
    assert_eq!(restored.temperature, Some(0.5));
}
