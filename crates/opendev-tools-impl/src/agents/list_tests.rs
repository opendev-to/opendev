use super::*;

#[tokio::test]
async fn test_list_agents_default() {
    let tool = AgentsTool;
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("Available agents"));
    assert!(output.contains("explore"));
    assert!(output.contains("planner"));
}

#[tokio::test]
async fn test_list_agents_explicit_action() {
    let tool = AgentsTool;
    let ctx = ToolContext::new("/tmp");
    let mut args = HashMap::new();
    args.insert("action".to_string(), serde_json::json!("list"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
}

#[tokio::test]
async fn test_list_agents_unknown_action() {
    let tool = AgentsTool;
    let ctx = ToolContext::new("/tmp");
    let mut args = HashMap::new();
    args.insert("action".to_string(), serde_json::json!("spawn"));
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Unknown action"));
}

#[tokio::test]
async fn test_list_agents_with_custom_context() {
    let tool = AgentsTool;
    let custom_agents = serde_json::json!([
        {
            "name": "custom_agent",
            "description": "A custom agent",
            "tools": ["read_file", "write_file"]
        }
    ]);
    let ctx = ToolContext::new("/tmp").with_value("agent_types", custom_agents);
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("custom_agent"));
    assert!(output.contains("A custom agent"));
}

#[test]
fn test_default_agent_types() {
    let agents = default_agent_types();
    assert!(agents.len() >= 3);

    let names: Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();
    assert!(names.contains(&"explore"));
    assert!(names.contains(&"planner"));
    assert!(names.contains(&"ask_user"));
}
