use super::*;
use crate::subagents::spec::PermissionAction;

#[test]
fn test_load_custom_agent_md() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("my-reviewer.md"),
        "---\ndescription: \"Reviews code\"\ntools:\n  - read_file\n  - search\n---\n\nYou are a code reviewer.\n",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].name, "my-reviewer");
    assert!(specs[0].tools.contains(&"read_file".to_string()));
    assert!(specs[0].tools.contains(&"search".to_string()));
    assert!(specs[0].system_prompt.contains("code reviewer"));
}

#[test]
fn test_load_disabled_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("disabled.md"),
        "---\ndisabled: true\n---\nShould not load.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert!(specs.is_empty());
}

#[test]
fn test_load_primary_mode_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("primary-only.md"),
        "---\nmode: primary\n---\nPrimary mode agent.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert!(specs.is_empty());
}

#[test]
fn test_load_no_frontmatter() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(agent_dir.join("simple.md"), "You are a simple agent.").unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].name, "simple");
    assert!(specs[0].system_prompt.contains("simple agent"));
    assert!(!specs[0].has_tool_restriction());
}

#[test]
fn test_load_nonexistent_dir() {
    let specs = load_custom_agents(&[PathBuf::from("/nonexistent/path/agents")]);
    assert!(specs.is_empty());
}

#[test]
fn test_load_with_model_override() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("fast.md"),
        "---\nmodel: gpt-4o-mini\n---\nFast agent.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].model.as_deref(), Some("gpt-4o-mini"));
}

#[test]
fn test_load_with_extended_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("custom.md"),
        "---\ndescription: Custom agent\nhidden: true\nsteps: 50\ntemperature: 0.3\n---\nYou are custom.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert!(specs[0].hidden);
    assert_eq!(specs[0].max_steps, Some(50));
    assert_eq!(specs[0].temperature, Some(0.3));
}

#[test]
fn test_load_with_top_p() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("precise.md"),
        "---\ndescription: Precise agent\ntop_p: 0.9\n---\nPrecise agent.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].top_p, Some(0.9));
}

#[test]
fn test_load_with_mode_all() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("versatile.md"),
        "---\ndescription: Versatile\nmode: all\n---\nVersatile agent.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].mode, crate::subagents::AgentMode::All);
}

#[test]
fn test_load_with_color() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("colorful.md"),
        "---\ndescription: Colorful agent\ncolor: \"#38A3EE\"\n---\nColorful agent.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].color.as_deref(), Some("#38A3EE"));
}

#[test]
fn test_load_with_max_tokens() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("verbose.md"),
        "---\ndescription: Verbose agent\nmax_tokens: 8192\n---\nVerbose agent.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].max_tokens, Some(8192));
}

#[test]
fn test_load_with_max_tokens_camel_case() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("verbose2.md"),
        "---\nmaxTokens: 16384\n---\nVerbose agent.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].max_tokens, Some(16384));
}

#[test]
fn test_load_with_max_steps_alias() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("agent.md"),
        "---\nmaxSteps: 100\n---\nAgent body.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].max_steps, Some(100));
}

#[test]
fn test_load_recursive_nested_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    let nested = agent_dir.join("review");
    std::fs::create_dir_all(&nested).unwrap();

    // Top-level agent
    std::fs::write(
        agent_dir.join("top.md"),
        "---\ndescription: Top agent\n---\nTop agent prompt.",
    )
    .unwrap();

    // Nested agent
    std::fs::write(
        nested.join("deep.md"),
        "---\ndescription: Deep agent\n---\nDeep agent prompt.",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 2);
    let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"top"));
    assert!(names.contains(&"deep"));
}

#[test]
fn test_load_agent_with_permission() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        agent_dir.join("restricted.md"),
        "---\ndescription: Restricted agent\npermission:\n  edit: deny\n  bash: ask\n---\n\nRestricted agent.\n",
    )
    .unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(
        specs[0].evaluate_permission("edit", "any_file"),
        Some(PermissionAction::Deny)
    );
    assert_eq!(
        specs[0].evaluate_permission("bash", "any_command"),
        Some(PermissionAction::Ask)
    );
}

#[test]
fn test_load_recursive_skips_non_md() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().join("agents");
    let nested = agent_dir.join("sub");
    std::fs::create_dir_all(&nested).unwrap();

    std::fs::write(
        agent_dir.join("valid.md"),
        "---\ndescription: Valid\n---\nValid.",
    )
    .unwrap();
    std::fs::write(nested.join("config.json"), r#"{"key": "val"}"#).unwrap();
    std::fs::write(nested.join("notes.txt"), "not an agent").unwrap();

    let specs = load_custom_agents(&[agent_dir]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].name, "valid");
}
