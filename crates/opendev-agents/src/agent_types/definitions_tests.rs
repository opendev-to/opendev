use super::*;

#[test]
fn test_agent_definition_from_role() {
    let def = AgentDefinition::from_role(AgentRole::Code);
    assert_eq!(def.role, AgentRole::Code);
    assert!(def.system_prompt.is_none());
}

#[test]
fn test_agent_definition_effective_system_prompt() {
    let def = AgentDefinition::from_role(AgentRole::Code);
    assert!(def.effective_system_prompt().contains("coding agent"));
    let def = def.with_system_prompt("Custom prompt");
    assert_eq!(def.effective_system_prompt(), "Custom prompt");
}

#[test]
fn test_agent_definition_is_tool_allowed() {
    let code = AgentDefinition::from_role(AgentRole::Code);
    assert!(code.is_tool_allowed("bash"));
    assert!(code.is_tool_allowed("anything"));
    let plan = AgentDefinition::from_role(AgentRole::Plan);
    assert!(plan.is_tool_allowed("read_file"));
    assert!(!plan.is_tool_allowed("bash"));
}

#[test]
fn test_agent_definition_filter_tool_schemas() {
    let schemas = vec![
        serde_json::json!({"function": {"name": "read_file"}}),
        serde_json::json!({"function": {"name": "bash"}}),
        serde_json::json!({"function": {"name": "grep"}}),
    ];
    let code = AgentDefinition::from_role(AgentRole::Code);
    assert_eq!(code.filter_tool_schemas(&schemas).len(), 3);
    let plan = AgentDefinition::from_role(AgentRole::Plan);
    let filtered = plan.filter_tool_schemas(&schemas);
    assert_eq!(filtered.len(), 2);
    assert!(
        filtered
            .iter()
            .all(|s| s["function"]["name"].as_str().unwrap() != "bash")
    );
}

#[test]
fn test_agent_definition_with_tools() {
    let def = AgentDefinition::from_role(AgentRole::Code)
        .with_tools(vec!["read_file".into(), "bash".into()]);
    assert!(def.is_tool_allowed("read_file"));
    assert!(def.is_tool_allowed("bash"));
    assert!(!def.is_tool_allowed("write_file"));
}

#[test]
fn test_agent_definition_serialization() {
    let def = AgentDefinition::from_role(AgentRole::Test);
    let json = serde_json::to_string(&def).unwrap();
    let roundtrip: AgentDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.role, AgentRole::Test);
}
