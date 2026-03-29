use super::*;

fn make_spec(name: &str) -> SubAgentSpec {
    SubAgentSpec::new(name, format!("Description of {name}"), "system prompt")
}

#[test]
fn test_manager_new_empty() {
    let mgr = SubagentManager::new();
    assert!(mgr.is_empty());
    assert_eq!(mgr.len(), 0);
}

#[test]
fn test_register_and_get() {
    let mut mgr = SubagentManager::new();
    mgr.register(make_spec("Explore"));
    assert_eq!(mgr.len(), 1);
    assert!(mgr.get("Explore").is_some());
    assert!(mgr.get("nonexistent").is_none());
}

#[test]
fn test_get_by_type() {
    let mut mgr = SubagentManager::new();
    mgr.register(make_spec("Explore"));
    assert!(mgr.get_by_type(SubagentType::CodeExplorer).is_some());
    assert!(mgr.get_by_type(SubagentType::Planner).is_none());
}

#[test]
fn test_unregister() {
    let mut mgr = SubagentManager::new();
    mgr.register(make_spec("Planner"));
    assert!(mgr.unregister("Planner").is_some());
    assert!(mgr.is_empty());
}

#[test]
fn test_names() {
    let mut mgr = SubagentManager::new();
    mgr.register(make_spec("A"));
    mgr.register(make_spec("B"));
    let names = mgr.names();
    assert!(names.contains(&"A"));
    assert!(names.contains(&"B"));
}

#[test]
fn test_build_enum_description() {
    let mut mgr = SubagentManager::new();
    mgr.register(make_spec("Explore"));
    let descs = mgr.build_enum_description();
    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].0, "Explore");
}

#[test]
fn test_with_builtins() {
    let mgr = SubagentManager::with_builtins();
    assert_eq!(mgr.len(), 5);
    assert!(mgr.get("Explore").is_some());
    assert!(mgr.get("Planner").is_some());
    assert!(mgr.get("General").is_some());
    assert!(mgr.get("Build").is_some());
    assert!(mgr.get("project_init").is_some());
}

#[test]
fn test_with_builtins_and_custom() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join(".opendev").join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("test-agent.md"),
        "---\ndescription: Test agent\n---\nYou are a test.",
    )
    .unwrap();

    let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
    assert!(mgr.len() >= 4); // 3 builtins + 1 custom
    assert!(mgr.get("test-agent").is_some());
}

#[test]
fn test_dot_agents_directory_not_loaded() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join(".agents").join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("reviewer.md"),
        "---\ndescription: Code reviewer from .agents\n---\nYou review code.",
    )
    .unwrap();

    let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
    // .agents/ agents should not be loaded
    assert!(mgr.get("reviewer").is_none());
}

#[test]
fn test_only_opendev_agents_dir_loaded() {
    let tmp = tempfile::tempdir().unwrap();

    // Create agents in .opendev, .agents, and .claude dirs
    let opendev_dir = tmp.path().join(".opendev").join("agents");
    let agents_dir = tmp.path().join(".agents").join("agents");
    let claude_dir = tmp.path().join(".claude").join("agents");
    std::fs::create_dir_all(&opendev_dir).unwrap();
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::create_dir_all(&claude_dir).unwrap();

    std::fs::write(
        opendev_dir.join("shared.md"),
        "---\ndescription: From .opendev\n---\nOpenDev version.",
    )
    .unwrap();
    std::fs::write(
        agents_dir.join("shared.md"),
        "---\ndescription: From .agents\n---\nAgents version.",
    )
    .unwrap();
    std::fs::write(
        claude_dir.join("only-claude.md"),
        "---\ndescription: From .claude\n---\nClaude version.",
    )
    .unwrap();

    let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
    let spec = mgr.get("shared").unwrap();
    // Only .opendev is loaded
    assert_eq!(spec.description, "From .opendev");
    // .claude and .agents agents should not be loaded
    assert!(mgr.get("only-claude").is_none());
}

#[test]
fn test_hidden_agents_excluded_from_names() {
    let mut mgr = SubagentManager::new();
    mgr.register(make_spec("visible"));
    mgr.register(SubAgentSpec::new("hidden-agent", "Hidden", "prompt").with_hidden(true));

    let names = mgr.names();
    assert!(names.contains(&"visible"));
    assert!(!names.contains(&"hidden-agent"));

    // all_names includes hidden
    let all = mgr.all_names();
    assert!(all.contains(&"visible"));
    assert!(all.contains(&"hidden-agent"));
}

#[test]
fn test_hidden_agents_excluded_from_enum_description() {
    let mut mgr = SubagentManager::new();
    mgr.register(make_spec("visible"));
    mgr.register(SubAgentSpec::new("hidden-agent", "Hidden", "prompt").with_hidden(true));

    let descs = mgr.build_enum_description();
    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].0, "visible");
}

#[test]
fn test_hidden_agents_still_gettable() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("hidden-agent", "Hidden", "prompt").with_hidden(true));

    // Hidden agents can still be retrieved by name (for programmatic spawning)
    assert!(mgr.get("hidden-agent").is_some());
}

#[test]
fn test_custom_agent_with_steps_and_temperature() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join(".opendev").join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("custom.md"),
        "---\ndescription: Custom\nsteps: 50\ntemperature: 0.3\nhidden: true\n---\nYou are custom.",
    )
    .unwrap();

    let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
    let spec = mgr.get("custom").unwrap();
    assert_eq!(spec.max_steps, Some(50));
    assert_eq!(spec.temperature, Some(0.3));
    assert!(spec.hidden);

    // Hidden agent excluded from names
    assert!(!mgr.names().contains(&"custom"));
}

#[test]
fn test_custom_agent_overrides_builtin() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join(".opendev").join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    // Create a custom agent with same name as a builtin
    std::fs::write(
        agent_dir.join("Explore.md"),
        "---\ndescription: Custom explorer\ntemperature: 0.1\n---\nCustom explorer prompt.",
    )
    .unwrap();

    let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
    let spec = mgr.get("Explore").unwrap();
    // Custom should override the builtin
    assert!(spec.system_prompt.contains("Custom explorer prompt"));
    assert_eq!(spec.temperature, Some(0.1));
}

#[test]
fn test_claude_agents_dir_not_loaded() {
    let tmp = tempfile::tempdir().unwrap();
    let claude_dir = tmp.path().join(".claude").join("agents");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(
        claude_dir.join("claude-agent.md"),
        "---\ndescription: Claude agent\n---\nClaude agent prompt.",
    )
    .unwrap();

    let mgr = SubagentManager::with_builtins_and_custom(tmp.path());
    // .claude/ agents should not be loaded
    assert!(mgr.get("claude-agent").is_none());
}

// ---- Disabled agent filtering ----

#[test]
fn test_disabled_agents_excluded_from_names() {
    let mut mgr = SubagentManager::new();
    mgr.register(make_spec("active"));
    mgr.register(SubAgentSpec::new("disabled-agent", "Disabled", "prompt").with_disable(true));

    let names = mgr.names();
    assert!(names.contains(&"active"));
    assert!(
        !names.contains(&"disabled-agent"),
        "Disabled agents should be excluded from names()"
    );
}

#[test]
fn test_disabled_agents_excluded_from_enum_description() {
    let mut mgr = SubagentManager::new();
    mgr.register(make_spec("active"));
    mgr.register(SubAgentSpec::new("disabled-agent", "Disabled", "prompt").with_disable(true));

    let descs = mgr.build_enum_description();
    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].0, "active");
}

#[test]
fn test_disabled_agents_still_gettable() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("disabled-agent", "Disabled", "prompt").with_disable(true));

    // Disabled agents can still be looked up (but spawn will fail)
    assert!(mgr.get("disabled-agent").is_some());
}

#[test]
fn test_disabled_agents_in_all_names() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("disabled-agent", "Disabled", "prompt").with_disable(true));

    let all = mgr.all_names();
    assert!(
        all.contains(&"disabled-agent"),
        "all_names() should include disabled agents"
    );
}

// ---- apply_config_overrides tests ----

#[test]
fn test_config_override_model() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("build", "Build agent", "Build things."));

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "build".to_string(),
        opendev_models::AgentConfigInline {
            model: Some("gpt-4o-mini".to_string()),
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    let spec = mgr.get("build").unwrap();
    assert_eq!(spec.model.as_deref(), Some("gpt-4o-mini"));
}

#[test]
fn test_config_override_temperature_and_top_p() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("test", "Test", "prompt"));

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "test".to_string(),
        opendev_models::AgentConfigInline {
            temperature: Some(0.7),
            top_p: Some(0.9),
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    let spec = mgr.get("test").unwrap();
    assert!((spec.temperature.unwrap() - 0.7).abs() < 0.01);
    assert!((spec.top_p.unwrap() - 0.9).abs() < 0.01);
}

#[test]
fn test_config_override_disable_removes_agent() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("build", "Build agent", "prompt"));
    assert!(mgr.get("build").is_some());

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "build".to_string(),
        opendev_models::AgentConfigInline {
            disable: Some(true),
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    assert!(
        mgr.get("build").is_none(),
        "Disabled agent should be removed"
    );
}

#[test]
fn test_config_override_creates_new_agent() {
    let mut mgr = SubagentManager::new();
    assert!(mgr.get("custom-agent").is_none());

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "custom-agent".to_string(),
        opendev_models::AgentConfigInline {
            description: Some("My custom agent".to_string()),
            prompt: Some("Be creative.".to_string()),
            model: Some("claude-opus-4-5".to_string()),
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    let spec = mgr.get("custom-agent").unwrap();
    assert_eq!(spec.description, "My custom agent");
    assert_eq!(spec.system_prompt, "Be creative.");
    assert_eq!(spec.model.as_deref(), Some("claude-opus-4-5"));
}

#[test]
fn test_config_override_prompt_and_description() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("build", "Old desc", "Old prompt"));

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "build".to_string(),
        opendev_models::AgentConfigInline {
            description: Some("New description".to_string()),
            prompt: Some("New system prompt".to_string()),
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    let spec = mgr.get("build").unwrap();
    assert_eq!(spec.description, "New description");
    assert_eq!(spec.system_prompt, "New system prompt");
}

#[test]
fn test_config_override_max_steps_and_color() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("build", "Build", "prompt"));

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "build".to_string(),
        opendev_models::AgentConfigInline {
            max_steps: Some(50),
            color: Some("#FF6600".to_string()),
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    let spec = mgr.get("build").unwrap();
    assert_eq!(spec.max_steps, Some(50));
    assert_eq!(spec.color.as_deref(), Some("#FF6600"));
}

#[test]
fn test_config_override_hidden() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("build", "Build", "prompt"));

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "build".to_string(),
        opendev_models::AgentConfigInline {
            hidden: Some(true),
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    let spec = mgr.get("build").unwrap();
    assert!(spec.hidden);
    assert!(!mgr.names().contains(&"build"));
}

#[test]
fn test_config_override_mode() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("build", "Build", "prompt"));

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "build".to_string(),
        opendev_models::AgentConfigInline {
            mode: Some("primary".to_string()),
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    let spec = mgr.get("build").unwrap();
    assert!(spec.mode.can_be_primary());
}

#[test]
fn test_config_override_permission_rules() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("build", "Build", "prompt"));

    let mut perms = std::collections::HashMap::new();
    perms.insert("bash".to_string(), "deny".to_string());
    perms.insert("edit".to_string(), "allow".to_string());

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "build".to_string(),
        opendev_models::AgentConfigInline {
            permission: perms,
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    let spec = mgr.get("build").unwrap();
    // Check bash is denied
    let bash_action = spec.evaluate_permission("bash", "");
    assert_eq!(bash_action, Some(crate::subagents::PermissionAction::Deny));
    // Check edit is allowed
    let edit_action = spec.evaluate_permission("edit", "");
    assert_eq!(edit_action, Some(crate::subagents::PermissionAction::Allow));
}

#[test]
fn test_config_override_invalid_permission_action_skipped() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("build", "Build", "prompt"));

    let mut perms = std::collections::HashMap::new();
    perms.insert("bash".to_string(), "invalid_action".to_string());

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "build".to_string(),
        opendev_models::AgentConfigInline {
            permission: perms,
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    let spec = mgr.get("build").unwrap();
    // Invalid action should be skipped, so no permission rule for bash
    assert_eq!(spec.evaluate_permission("bash", ""), None);
}

#[test]
fn test_config_override_multiple_agents() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("build", "Build", "prompt1"));
    mgr.register(SubAgentSpec::new("explore", "Explore", "prompt2"));

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "build".to_string(),
        opendev_models::AgentConfigInline {
            model: Some("gpt-4o".to_string()),
            ..Default::default()
        },
    );
    overrides.insert(
        "explore".to_string(),
        opendev_models::AgentConfigInline {
            temperature: Some(0.2),
            ..Default::default()
        },
    );
    mgr.apply_config_overrides(&overrides);

    assert_eq!(mgr.get("build").unwrap().model.as_deref(), Some("gpt-4o"));
    assert!((mgr.get("explore").unwrap().temperature.unwrap() - 0.2).abs() < 0.01);
}

// ---- resolve_default_agent tests ----

#[test]
fn test_resolve_default_agent_configured() {
    let mut mgr = SubagentManager::new();
    mgr.register(
        SubAgentSpec::new("build", "Build", "prompt").with_mode(crate::subagents::AgentMode::All),
    );

    let result = mgr.resolve_default_agent(Some("build"));
    assert_eq!(result, Some("build"));
}

#[test]
fn test_resolve_default_agent_not_found_falls_back() {
    let mut mgr = SubagentManager::new();
    mgr.register(
        SubAgentSpec::new("build", "Build", "prompt").with_mode(crate::subagents::AgentMode::All),
    );

    let result = mgr.resolve_default_agent(Some("nonexistent"));
    assert_eq!(
        result,
        Some("build"),
        "Should fall back to first primary-capable agent"
    );
}

#[test]
fn test_resolve_default_agent_disabled_falls_back() {
    let mut mgr = SubagentManager::new();
    mgr.register(
        SubAgentSpec::new("build", "Build", "prompt")
            .with_mode(crate::subagents::AgentMode::All)
            .with_disable(true),
    );
    mgr.register(
        SubAgentSpec::new("general", "General", "prompt")
            .with_mode(crate::subagents::AgentMode::Primary),
    );

    let result = mgr.resolve_default_agent(Some("build"));
    assert_eq!(
        result,
        Some("general"),
        "Should skip disabled and fall back"
    );
}

#[test]
fn test_resolve_default_agent_hidden_falls_back() {
    let mut mgr = SubagentManager::new();
    let mut hidden = SubAgentSpec::new("hidden-agent", "Hidden", "prompt");
    hidden.hidden = true;
    hidden.mode = crate::subagents::AgentMode::All;
    mgr.register(hidden);
    mgr.register(
        SubAgentSpec::new("visible", "Visible", "prompt")
            .with_mode(crate::subagents::AgentMode::Primary),
    );

    let result = mgr.resolve_default_agent(Some("hidden-agent"));
    assert_eq!(result, Some("visible"), "Should skip hidden and fall back");
}

#[test]
fn test_resolve_default_agent_subagent_only_falls_back() {
    let mut mgr = SubagentManager::new();
    mgr.register(SubAgentSpec::new("helper", "Helper", "prompt"));
    // Default mode is Subagent, which can't be primary
    mgr.register(
        SubAgentSpec::new("primary", "Primary", "prompt")
            .with_mode(crate::subagents::AgentMode::Primary),
    );

    let result = mgr.resolve_default_agent(Some("helper"));
    assert_eq!(
        result,
        Some("primary"),
        "Should skip subagent-only and fall back"
    );
}

#[test]
fn test_resolve_default_agent_none_configured() {
    let mut mgr = SubagentManager::new();
    mgr.register(
        SubAgentSpec::new("build", "Build", "prompt").with_mode(crate::subagents::AgentMode::All),
    );

    let result = mgr.resolve_default_agent(None);
    assert_eq!(
        result,
        Some("build"),
        "Should return first primary-capable agent"
    );
}

#[test]
fn test_resolve_default_agent_no_primary_capable() {
    let mut mgr = SubagentManager::new();
    // Only subagent-mode agents
    mgr.register(SubAgentSpec::new("helper1", "Helper 1", "prompt"));
    mgr.register(SubAgentSpec::new("helper2", "Helper 2", "prompt"));

    let result = mgr.resolve_default_agent(None);
    assert_eq!(result, None, "No primary-capable agents → None");
}
