use super::*;

#[tokio::test]
async fn test_spawn_subagent_missing_params() {
    let manager = Arc::new(opendev_agents::SubagentManager::new());
    let registry = Arc::new(opendev_tools_core::ToolRegistry::new());
    let raw = opendev_http::HttpClient::new(
        "https://api.example.com/v1/chat/completions",
        reqwest::header::HeaderMap::new(),
        None,
    )
    .unwrap();
    let http = Arc::new(opendev_http::AdaptedClient::new(raw));
    let tool = SpawnSubagentTool::new(
        manager,
        registry,
        http,
        PathBuf::from("/tmp"),
        "gpt-4o",
        "/tmp",
    );
    let ctx = ToolContext::new("/tmp");

    // Missing agent_type
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("agent_type"));

    // Missing task
    let mut args = HashMap::new();
    args.insert("agent_type".into(), serde_json::json!("code_explorer"));
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("task"));
}

#[tokio::test]
async fn test_spawn_subagent_unknown_type() {
    let manager = Arc::new(opendev_agents::SubagentManager::new());
    let registry = Arc::new(opendev_tools_core::ToolRegistry::new());
    let raw = opendev_http::HttpClient::new(
        "https://api.example.com/v1/chat/completions",
        reqwest::header::HeaderMap::new(),
        None,
    )
    .unwrap();
    let http = Arc::new(opendev_http::AdaptedClient::new(raw));
    let tool = SpawnSubagentTool::new(
        manager,
        registry,
        http,
        PathBuf::from("/tmp"),
        "gpt-4o",
        "/tmp",
    );
    let ctx = ToolContext::new("/tmp");

    let mut args = HashMap::new();
    args.insert("agent_type".into(), serde_json::json!("nonexistent"));
    args.insert("task".into(), serde_json::json!("do something"));
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Unknown subagent type"));
}

#[tokio::test]
async fn test_spawn_subagent_blocked_in_subagent_context() {
    let manager = Arc::new(opendev_agents::SubagentManager::new());
    let registry = Arc::new(opendev_tools_core::ToolRegistry::new());
    let raw = opendev_http::HttpClient::new(
        "https://api.example.com/v1/chat/completions",
        reqwest::header::HeaderMap::new(),
        None,
    )
    .unwrap();
    let http = Arc::new(opendev_http::AdaptedClient::new(raw));
    let tool = SpawnSubagentTool::new(
        manager,
        registry,
        http,
        PathBuf::from("/tmp"),
        "gpt-4o",
        "/tmp",
    );

    // Simulate being called from within a subagent context
    let mut ctx = ToolContext::new("/tmp");
    ctx.is_subagent = true;

    let mut args = HashMap::new();
    args.insert("agent_type".into(), serde_json::json!("code_explorer"));
    args.insert("task".into(), serde_json::json!("explore code"));

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(
        result
            .error
            .unwrap()
            .contains("cannot spawn other subagents")
    );
}

#[tokio::test]
async fn test_planner_blocked_during_explore_phase() {
    let mut manager = opendev_agents::SubagentManager::new();
    // Register a planner spec so agent_type validation passes
    manager.register(opendev_agents::SubAgentSpec::new(
        "Planner",
        "Plans tasks",
        "",
    ));
    let manager = Arc::new(manager);
    let registry = Arc::new(opendev_tools_core::ToolRegistry::new());
    let raw = opendev_http::HttpClient::new(
        "https://api.example.com/v1/chat/completions",
        reqwest::header::HeaderMap::new(),
        None,
    )
    .unwrap();
    let http = Arc::new(opendev_http::AdaptedClient::new(raw));
    let tool = SpawnSubagentTool::new(
        manager,
        registry,
        http,
        PathBuf::from("/tmp"),
        "gpt-4o",
        "/tmp",
    );

    // Create shared state in explore phase
    let mut state = HashMap::new();
    state.insert("planning_phase".to_string(), serde_json::json!("explore"));
    state.insert("explore_count".to_string(), serde_json::json!(0));
    let shared = std::sync::Arc::new(std::sync::Mutex::new(state));

    let mut ctx = ToolContext::new("/tmp");
    ctx.shared_state = Some(shared);

    let mut args = HashMap::new();
    args.insert("agent_type".into(), serde_json::json!("Planner"));
    args.insert("task".into(), serde_json::json!("plan the task"));

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Before planning"));
}

#[tokio::test]
async fn test_planner_allowed_during_plan_phase() {
    let mut manager = opendev_agents::SubagentManager::new();
    manager.register(opendev_agents::SubAgentSpec::new(
        "Planner",
        "Plans tasks",
        "",
    ));
    let manager = Arc::new(manager);
    let registry = Arc::new(opendev_tools_core::ToolRegistry::new());
    let raw = opendev_http::HttpClient::new(
        "https://api.example.com/v1/chat/completions",
        reqwest::header::HeaderMap::new(),
        None,
    )
    .unwrap();
    let http = Arc::new(opendev_http::AdaptedClient::new(raw));
    // Use non-existent working dir so spawn fails fast at dir validation
    // (never reaches network call)
    let tool = SpawnSubagentTool::new(
        manager,
        registry,
        http,
        PathBuf::from("/tmp"),
        "gpt-4o",
        "/nonexistent/path/for/test",
    );

    // Create shared state in plan phase (exploration already done)
    let mut state = HashMap::new();
    state.insert("planning_phase".to_string(), serde_json::json!("plan"));
    state.insert("explore_count".to_string(), serde_json::json!(1));
    let shared = std::sync::Arc::new(std::sync::Mutex::new(state));

    let mut ctx = ToolContext::new("/tmp");
    ctx.shared_state = Some(shared);

    let mut args = HashMap::new();
    args.insert("agent_type".into(), serde_json::json!("Planner"));
    args.insert("task".into(), serde_json::json!("plan the task"));

    // Should NOT be blocked by the explore guard — will fail at dir validation
    // Use explicit non-existent working_dir arg to trigger fast failure
    args.insert(
        "working_dir".into(),
        serde_json::json!("/nonexistent/test/path"),
    );
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(
        !err.contains("Before planning"),
        "Planner should not be blocked in plan phase, got: {err}"
    );
    assert!(
        err.contains("does not exist"),
        "Expected dir validation error, got: {err}"
    );
}

#[tokio::test]
async fn test_planner_allowed_without_shared_state() {
    let mut manager = opendev_agents::SubagentManager::new();
    manager.register(opendev_agents::SubAgentSpec::new(
        "Planner",
        "Plans tasks",
        "",
    ));
    let manager = Arc::new(manager);
    let registry = Arc::new(opendev_tools_core::ToolRegistry::new());
    let raw = opendev_http::HttpClient::new(
        "https://api.example.com/v1/chat/completions",
        reqwest::header::HeaderMap::new(),
        None,
    )
    .unwrap();
    let http = Arc::new(opendev_http::AdaptedClient::new(raw));
    let tool = SpawnSubagentTool::new(
        manager,
        registry,
        http,
        PathBuf::from("/tmp"),
        "gpt-4o",
        "/tmp",
    );

    // No shared state — normal non-plan-mode usage
    let ctx = ToolContext::new("/tmp");

    let mut args = HashMap::new();
    args.insert("agent_type".into(), serde_json::json!("Planner"));
    args.insert("task".into(), serde_json::json!("plan the task"));
    args.insert(
        "working_dir".into(),
        serde_json::json!("/nonexistent/test/path"),
    );

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(
        !err.contains("Before planning"),
        "Planner should not be blocked without shared state, got: {err}"
    );
}
