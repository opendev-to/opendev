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
fn test_parse_simple_yaml_scalar() {
    let text = "name: commit\ndescription: \"Git commit\"\n# comment\nnamespace: git";
    let data = parse_simple_yaml(text);
    assert_eq!(data.get("name").unwrap().as_scalar().unwrap(), "commit");
    assert_eq!(
        data.get("description").unwrap().as_scalar().unwrap(),
        "Git commit"
    );
    assert_eq!(data.get("namespace").unwrap().as_scalar().unwrap(), "git");
}

#[test]
fn test_parse_simple_yaml_single_quotes() {
    let text = "name: 'my-skill'";
    let data = parse_simple_yaml(text);
    assert_eq!(data.get("name").unwrap().as_scalar().unwrap(), "my-skill");
}

#[test]
fn test_parse_simple_yaml_inline_array() {
    let text = "paths: [\"**/*.rs\", \"**/*.ts\"]";
    let data = parse_simple_yaml(text);
    let paths = data.get("paths").unwrap().as_list();
    assert_eq!(paths, vec!["**/*.rs", "**/*.ts"]);
}

#[test]
fn test_parse_simple_yaml_inline_array_unquoted() {
    let text = "allowed-tools: [Bash, Edit, Read]";
    let data = parse_simple_yaml(text);
    let tools = data.get("allowed-tools").unwrap().as_list();
    assert_eq!(tools, vec!["Bash", "Edit", "Read"]);
}

#[test]
fn test_parse_simple_yaml_block_array() {
    let text = "allowed-tools:\n  - Bash\n  - Edit\n  - Read";
    let data = parse_simple_yaml(text);
    let tools = data.get("allowed-tools").unwrap().as_list();
    assert_eq!(tools, vec!["Bash", "Edit", "Read"]);
}

#[test]
fn test_parse_simple_yaml_block_array_with_blank_lines() {
    let text = "paths:\n  - \"**/*.rs\"\n\n  - \"**/*.ts\"\nname: test";
    let data = parse_simple_yaml(text);
    let paths = data.get("paths").unwrap().as_list();
    assert_eq!(paths, vec!["**/*.rs", "**/*.ts"]);
    assert_eq!(data.get("name").unwrap().as_scalar().unwrap(), "test");
}

#[test]
fn test_parse_simple_yaml_empty_value_no_list() {
    let text = "hooks:\nname: test";
    let data = parse_simple_yaml(text);
    // "hooks:" with no list items becomes empty scalar
    assert_eq!(data.get("hooks").unwrap().as_scalar().unwrap(), "");
    assert_eq!(data.get("name").unwrap().as_scalar().unwrap(), "test");
}

#[test]
fn test_parse_simple_yaml_bool_values() {
    let text = "disable-model-invocation: true\nuser-invocable: false";
    let data = parse_simple_yaml(text);
    assert_eq!(
        data.get("disable-model-invocation").unwrap().as_bool(),
        Some(true)
    );
    assert_eq!(data.get("user-invocable").unwrap().as_bool(), Some(false));
}

#[test]
fn test_frontmatter_value_as_list_from_scalar() {
    let v = FrontmatterValue::Scalar("a, b, c".to_string());
    assert_eq!(v.as_list(), vec!["a", "b", "c"]);
}

#[test]
fn test_frontmatter_value_as_list_from_empty_scalar() {
    let v = FrontmatterValue::Scalar(String::new());
    assert!(v.as_list().is_empty());
}

#[test]
fn test_frontmatter_value_as_bool_variants() {
    assert_eq!(FrontmatterValue::Scalar("yes".into()).as_bool(), Some(true));
    assert_eq!(FrontmatterValue::Scalar("1".into()).as_bool(), Some(true));
    assert_eq!(FrontmatterValue::Scalar("no".into()).as_bool(), Some(false));
    assert_eq!(FrontmatterValue::Scalar("0".into()).as_bool(), Some(false));
    assert_eq!(FrontmatterValue::Scalar("maybe".into()).as_bool(), None);
    assert_eq!(FrontmatterValue::List(vec![]).as_bool(), None);
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

// ---- New fields: paths, context, effort, allowed-tools, visibility, hooks ----

#[test]
fn test_parse_frontmatter_with_paths_inline() {
    let content =
        "---\nname: ts-fix\ndescription: Fix TS\npaths: [\"**/*.ts\", \"**/*.tsx\"]\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.paths, vec!["**/*.ts", "**/*.tsx"]);
}

#[test]
fn test_parse_frontmatter_with_paths_block() {
    let content = "---\nname: ts-fix\ndescription: Fix TS\npaths:\n  - \"**/*.ts\"\n  - \"**/*.tsx\"\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.paths, vec!["**/*.ts", "**/*.tsx"]);
}

#[test]
fn test_parse_frontmatter_with_context_fork() {
    let content = "---\nname: heavy\ndescription: Heavy skill\ncontext: fork\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.context, SkillContext::Fork);
}

#[test]
fn test_parse_frontmatter_with_context_inline_default() {
    let content = "---\nname: light\ndescription: Light skill\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.context, SkillContext::Inline);
}

#[test]
fn test_parse_frontmatter_with_effort() {
    let content = "---\nname: deep\ndescription: Deep analysis\neffort: high\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.effort, SkillEffort::High);
}

#[test]
fn test_parse_frontmatter_with_allowed_tools() {
    let content = "---\nname: safe\ndescription: Safe skill\nallowed-tools: [Read, Grep, Glob]\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.allowed_tools, vec!["Read", "Grep", "Glob"]);
}

#[test]
fn test_parse_frontmatter_with_visibility() {
    let content = "---\nname: secret\ndescription: Secret skill\ndisable-model-invocation: true\nuser-invocable: true\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert!(meta.disable_model_invocation);
    assert!(meta.user_invocable);
}

#[test]
fn test_parse_frontmatter_visibility_defaults() {
    let content = "---\nname: normal\ndescription: Normal skill\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert!(!meta.disable_model_invocation);
    assert!(meta.user_invocable); // default true
}

#[test]
fn test_parse_frontmatter_with_hooks() {
    let content = "---\nname: checked\ndescription: Checked skill\nhooks: [\"PreToolUse:Edit:npx tsc --noEmit\", \"PostToolUse::echo done\"]\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.hooks.len(), 2);
    assert_eq!(meta.hooks[0].event, "PreToolUse");
    assert_eq!(meta.hooks[0].matcher.as_deref(), Some("Edit"));
    assert_eq!(meta.hooks[0].command, "npx tsc --noEmit");
    assert_eq!(meta.hooks[1].event, "PostToolUse");
    assert!(meta.hooks[1].matcher.is_none());
    assert_eq!(meta.hooks[1].command, "echo done");
}

#[test]
fn test_parse_frontmatter_no_hooks_is_empty() {
    let content = "---\nname: simple\ndescription: Simple\n---\n\nBody\n";
    let meta = parse_frontmatter_str(content).unwrap();
    assert!(meta.hooks.is_empty());
}

#[test]
fn test_parse_frontmatter_full_featured() {
    let content = "\
---
name: typescript-fixer
description: Fix TypeScript compilation errors
namespace: typescript
model: claude-sonnet-4-5-20250514
context: fork
effort: high
paths: [\"**/*.ts\", \"**/*.tsx\"]
allowed-tools: [Read, Edit, Bash, Grep]
user-invocable: true
disable-model-invocation: false
hooks: [\"PostToolUse:Edit:npx tsc --noEmit 2>&1 | head -20\"]
---

# TypeScript Fixer

Run tsc and fix all type errors.
";
    let meta = parse_frontmatter_str(content).unwrap();
    assert_eq!(meta.name, "typescript-fixer");
    assert_eq!(meta.namespace, "typescript");
    assert_eq!(meta.model.as_deref(), Some("claude-sonnet-4-5-20250514"));
    assert_eq!(meta.context, SkillContext::Fork);
    assert_eq!(meta.effort, SkillEffort::High);
    assert_eq!(meta.paths, vec!["**/*.ts", "**/*.tsx"]);
    assert_eq!(meta.allowed_tools, vec!["Read", "Edit", "Bash", "Grep"]);
    assert!(meta.user_invocable);
    assert!(!meta.disable_model_invocation);
    assert_eq!(meta.hooks.len(), 1);
    assert_eq!(meta.hooks[0].event, "PostToolUse");
    assert_eq!(meta.hooks[0].matcher.as_deref(), Some("Edit"));
}

// ---- SkillContext ----

#[test]
fn test_skill_context_from_str() {
    assert_eq!(
        SkillContext::from_str_opt("inline"),
        Some(SkillContext::Inline)
    );
    assert_eq!(SkillContext::from_str_opt("fork"), Some(SkillContext::Fork));
    assert_eq!(SkillContext::from_str_opt("FORK"), Some(SkillContext::Fork));
    assert_eq!(SkillContext::from_str_opt("invalid"), None);
}

// ---- SkillEffort ----

#[test]
fn test_skill_effort_from_str() {
    assert_eq!(SkillEffort::from_str_opt("low"), Some(SkillEffort::Low));
    assert_eq!(
        SkillEffort::from_str_opt("medium"),
        Some(SkillEffort::Medium)
    );
    assert_eq!(SkillEffort::from_str_opt("high"), Some(SkillEffort::High));
    assert_eq!(SkillEffort::from_str_opt("max"), Some(SkillEffort::Max));
    assert_eq!(SkillEffort::from_str_opt("HIGH"), Some(SkillEffort::High));
    assert_eq!(SkillEffort::from_str_opt("unknown"), None);
}

#[test]
fn test_skill_effort_max_steps() {
    assert_eq!(SkillEffort::Low.max_steps(), 10);
    assert_eq!(SkillEffort::Medium.max_steps(), 25);
    assert_eq!(SkillEffort::High.max_steps(), 50);
    assert_eq!(SkillEffort::Max.max_steps(), 100);
}

#[test]
fn test_skill_effort_reasoning() {
    assert_eq!(SkillEffort::Low.reasoning_effort(), "low");
    assert_eq!(SkillEffort::Medium.reasoning_effort(), "medium");
    assert_eq!(SkillEffort::High.reasoning_effort(), "high");
    assert_eq!(SkillEffort::Max.reasoning_effort(), "high");
}
