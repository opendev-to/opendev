use super::*;

// ---- Frontmatter parsing ----

#[test]
fn test_parse_frontmatter_basic() {
    let content = "---\nname: commit\ndescription: Git commit skill\n---\n\n# Commit\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.name, "commit");
    assert_eq!(meta.description, "Git commit skill");
    assert_eq!(meta.namespace, "default");
}

#[test]
fn test_parse_frontmatter_with_namespace() {
    let content = "---\nname: rebase\ndescription: Rebase skill\nnamespace: git\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.name, "rebase");
    assert_eq!(meta.namespace, "git");
}

#[test]
fn test_parse_frontmatter_quoted_values() {
    let content = "---\nname: \"my-skill\"\ndescription: 'Use when testing'\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.name, "my-skill");
    assert_eq!(meta.description, "Use when testing");
}

#[test]
fn test_parse_frontmatter_missing_returns_none() {
    let content = "# No frontmatter here\nJust a plain markdown file.\n";
    assert!(parse_frontmatter_str(content).is_none());
}

#[test]
fn test_parse_frontmatter_empty_name_fallback() {
    let content = "---\ndescription: Some skill\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert!(meta.name.is_empty()); // caller (parse_frontmatter_file) fills in
    assert_eq!(meta.description, "Some skill");
}

// ---- Strip frontmatter ----

#[test]
fn test_strip_frontmatter() {
    let content = "---\nname: foo\n---\n\n# Title\nBody text.";
    let body = strip_frontmatter(content);
    assert!(body.starts_with("# Title"));
    assert!(!body.contains("---"));
}

#[test]
fn test_strip_frontmatter_no_frontmatter() {
    let content = "# Just markdown\nNo frontmatter.";
    let body = strip_frontmatter(content);
    assert_eq!(body, content);
}

// ---- Simple YAML parser ----

#[test]
fn test_parse_simple_yaml() {
    let text = "name: commit\ndescription: \"Git commit\"\n# comment\nnamespace: git";
    let data = parse_simple_yaml(text);
    assert_eq!(data.get("name").unwrap(), "commit");
    assert_eq!(data.get("description").unwrap(), "Git commit");
    assert_eq!(data.get("namespace").unwrap(), "git");
}

#[test]
fn test_parse_simple_yaml_single_quotes() {
    let text = "name: 'my-skill'";
    let data = parse_simple_yaml(text);
    assert_eq!(data.get("name").unwrap(), "my-skill");
}

// ---- Model/agent in frontmatter ----

#[test]
fn test_parse_frontmatter_with_model() {
    let content =
        "---\nname: fast-review\ndescription: Quick review\nmodel: gpt-4o-mini\n---\n\n# Review\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.name, "fast-review");
    assert_eq!(meta.model.as_deref(), Some("gpt-4o-mini"));
}

#[test]
fn test_parse_frontmatter_with_agent() {
    let content = "---\nname: deploy\ndescription: Deploy skill\nagent: devops\n---\n\n# Deploy\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.name, "deploy");
    assert_eq!(meta.agent.as_deref(), Some("devops"));
}

#[test]
fn test_parse_frontmatter_no_agent_field() {
    let content = "---\nname: commit\ndescription: Git commit skill\n---\n\n# Commit\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert!(meta.agent.is_none());
}

#[test]
fn test_parse_frontmatter_no_model_field() {
    let content = "---\nname: commit\ndescription: Git commit skill\n---\n\n# Commit\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert!(meta.model.is_none());
}
