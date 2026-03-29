use super::*;

#[test]
fn test_agent_role_display() {
    assert_eq!(AgentRole::Code.to_string(), "Code");
    assert_eq!(AgentRole::Plan.to_string(), "Plan");
    assert_eq!(AgentRole::Test.to_string(), "Test");
    assert_eq!(AgentRole::Build.to_string(), "Build");
}

#[test]
fn test_agent_role_default_system_prompt() {
    assert!(
        AgentRole::Code
            .default_system_prompt()
            .contains("coding agent")
    );
    assert!(
        AgentRole::Plan
            .default_system_prompt()
            .contains("planning agent")
    );
    assert!(
        AgentRole::Test
            .default_system_prompt()
            .contains("testing agent")
    );
    assert!(
        AgentRole::Build
            .default_system_prompt()
            .contains("build agent")
    );
}

#[test]
fn test_agent_role_default_tools() {
    assert!(AgentRole::Code.default_tools().is_empty());
    let plan_tools = AgentRole::Plan.default_tools();
    assert!(plan_tools.contains(&"read_file".to_string()));
    assert!(!plan_tools.contains(&"bash".to_string()));
    assert!(
        AgentRole::Test
            .default_tools()
            .contains(&"bash".to_string())
    );
    assert!(
        AgentRole::Build
            .default_tools()
            .contains(&"bash".to_string())
    );
}
