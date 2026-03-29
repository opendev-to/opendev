use super::*;

#[test]
fn test_all_templates_embedded() {
    assert_eq!(TEMPLATES.len(), TEMPLATE_COUNT);
}

#[test]
fn test_get_embedded_known() {
    let content = get_embedded("system/main/main-security-policy.md");
    assert!(content.is_some());
    assert!(content.unwrap().contains("Security Policy"));
}

#[test]
fn test_get_embedded_unknown() {
    assert!(get_embedded("nonexistent.md").is_none());
}

#[test]
fn test_system_main_templates() {
    let templates = system_main_templates();
    assert!(templates.len() >= 20);
    assert!(templates.iter().all(|(k, _)| k.starts_with("system/main/")));
}

#[test]
fn test_tool_templates() {
    let templates = tool_templates();
    assert!(templates.len() >= 30);
    assert!(templates.iter().all(|(k, _)| k.starts_with("tools/")));
}

#[test]
fn test_subagent_templates() {
    let templates = subagent_templates();
    assert!(templates.len() >= 4);
}

#[test]
fn test_build_init_prompt_no_args() {
    let prompt = build_init_prompt("");
    assert!(prompt.contains("AGENTS.md"));
    assert!(prompt.contains("Build/lint/test"));
    assert!(!prompt.contains("{args}"));
}

#[test]
fn test_build_init_prompt_with_args() {
    let prompt = build_init_prompt("focus on testing");
    assert!(prompt.contains("focus on testing"));
    assert!(!prompt.contains("{args}"));
}

#[test]
fn test_no_empty_templates() {
    for (path, content) in TEMPLATES.iter() {
        assert!(!content.is_empty(), "Template {} is empty", path);
    }
}
