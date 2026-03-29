use super::super::types::SubAgentSpec;
use super::*;

#[test]
fn test_agent_mode_default() {
    assert_eq!(AgentMode::default(), AgentMode::Subagent);
}

#[test]
fn test_agent_mode_from_str() {
    assert_eq!(AgentMode::parse_mode("primary"), AgentMode::Primary);
    assert_eq!(AgentMode::parse_mode("subagent"), AgentMode::Subagent);
    assert_eq!(AgentMode::parse_mode("all"), AgentMode::All);
    assert_eq!(AgentMode::parse_mode("unknown"), AgentMode::Subagent);
}

#[test]
fn test_agent_mode_capabilities() {
    assert!(AgentMode::Primary.can_be_primary());
    assert!(!AgentMode::Primary.can_be_subagent());

    assert!(!AgentMode::Subagent.can_be_primary());
    assert!(AgentMode::Subagent.can_be_subagent());

    assert!(AgentMode::All.can_be_primary());
    assert!(AgentMode::All.can_be_subagent());
}

#[test]
fn test_agent_mode_serde() {
    let spec = SubAgentSpec::new("test", "desc", "prompt").with_mode(AgentMode::Primary);

    let json = serde_json::to_string(&spec).unwrap();
    assert!(json.contains("\"primary\""));

    let restored: SubAgentSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.mode, AgentMode::Primary);
}
