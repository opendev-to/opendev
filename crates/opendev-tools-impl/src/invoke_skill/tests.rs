use super::*;
use std::fs;
use tempfile::TempDir;

fn create_test_loader(skill_dir: Option<&std::path::Path>) -> Arc<Mutex<SkillLoader>> {
    let dirs = match skill_dir {
        Some(d) => vec![d.to_path_buf()],
        None => vec![],
    };
    let mut loader = SkillLoader::new(dirs);
    loader.discover_skills();
    Arc::new(Mutex::new(loader))
}

#[tokio::test]
async fn test_list_skills_no_arg() {
    let loader = create_test_loader(None);
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("Available skills:"));
    assert!(output.contains("commit"));
}

#[tokio::test]
async fn test_list_skills_empty_string() {
    let loader = create_test_loader(None);
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!(""));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("Available skills:"));
}

#[tokio::test]
async fn test_load_builtin_skill() {
    let loader = create_test_loader(None);
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("commit"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("Loaded skill: commit"));
    assert!(output.contains("Git Commit"));
    assert_eq!(result.metadata.get("skill_name").unwrap(), "commit");
    assert_eq!(result.metadata.get("skill_namespace").unwrap(), "default");
}

#[tokio::test]
async fn test_skill_not_found() {
    let loader = create_test_loader(None);
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert(
        "skill_name".to_string(),
        serde_json::json!("nonexistent-skill-xyz"),
    );
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    let error = result.error.unwrap();
    assert!(error.contains("Skill not found: 'nonexistent-skill-xyz'"));
    assert!(error.contains("Available skills:"));
}

#[tokio::test]
async fn test_subagent_type_redirects_to_spawn_subagent() {
    let loader = create_test_loader(None);
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    for name in &[
        "explore",
        "code-explorer",
        "code_explorer",
        "planner",
        "ask_user",
    ] {
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!(name));
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success, "Should fail for subagent type '{name}'");
        let error = result.error.unwrap();
        assert!(error.contains("subagent type, not a skill"));
        assert!(error.contains("spawn_subagent"));
    }
}

#[tokio::test]
async fn test_dedup_second_invoke_returns_reminder() {
    let loader = create_test_loader(None);
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("commit"));
    let result1 = tool.execute(args.clone(), &ctx).await;
    assert!(result1.success);
    assert!(result1.output.unwrap().contains("Loaded skill: commit"));
    let result2 = tool.execute(args, &ctx).await;
    assert!(result2.success);
    let output2 = result2.output.unwrap();
    assert!(output2.contains("already loaded"));
    assert!(output2.contains("do not invoke this skill again"));
}

#[tokio::test]
async fn test_load_filesystem_skill() {
    let tmp = TempDir::new().unwrap();
    let skill_dir = tmp.path().join("skills");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("deploy.md"),
        "---\nname: deploy\ndescription: Deploy instructions\nnamespace: ops\n---\n\n# Deploy\nStep 1: push.\n",
    ).unwrap();
    let loader = create_test_loader(Some(&skill_dir));
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("deploy"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("Loaded skill: deploy"));
    assert!(output.contains("Step 1: push."));
    assert_eq!(result.metadata.get("skill_namespace").unwrap(), "ops");
}

#[tokio::test]
async fn test_load_directory_skill_with_companions() {
    let tmp = TempDir::new().unwrap();
    let skill_dir = tmp.path().join("skills");
    let sub_dir = skill_dir.join("testing");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(
        sub_dir.join("SKILL.md"),
        "---\nname: testing\ndescription: Testing patterns\n---\n\n# Testing\nTest content.\n",
    )
    .unwrap();
    fs::write(sub_dir.join("helpers.sh"), "#!/bin/bash\necho test").unwrap();
    fs::write(sub_dir.join("config.json"), r#"{"key": "val"}"#).unwrap();
    let loader = create_test_loader(Some(&skill_dir));
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("testing"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("Loaded skill: testing"));
    assert!(output.contains("<skill_files>"));
    assert!(output.contains("helpers.sh"));
    assert!(output.contains("config.json"));
    assert!(output.contains("Base directory for this skill:"));
}

#[tokio::test]
async fn test_invoke_skill_with_arguments() {
    let tmp = TempDir::new().unwrap();
    let skill_dir = tmp.path().join("skills");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("greet.md"),
        "---\nname: greet\ndescription: Greet someone\n---\n\nHello $1, welcome to $2!\n",
    )
    .unwrap();
    let loader = create_test_loader(Some(&skill_dir));
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("greet"));
    args.insert("arguments".to_string(), serde_json::json!("Alice OpenDev"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(
        result
            .output
            .unwrap()
            .contains("Hello Alice, welcome to OpenDev!")
    );
}

#[tokio::test]
async fn test_invoke_skill_with_model_override() {
    let tmp = TempDir::new().unwrap();
    let skill_dir = tmp.path().join("skills");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("fast-lint.md"),
        "---\nname: fast-lint\ndescription: Fast lint\nmodel: gpt-4o-mini\n---\n\n# Lint\nDo fast linting.\n",
    ).unwrap();
    let loader = create_test_loader(Some(&skill_dir));
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("fast-lint"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert_eq!(result.metadata.get("skill_model").unwrap(), "gpt-4o-mini");
}

#[tokio::test]
async fn test_invoke_skill_without_model_no_metadata() {
    let loader = create_test_loader(None);
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("commit"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.metadata.get("skill_model").is_none());
}

#[tokio::test]
async fn test_invoke_skill_with_agent_override() {
    let tmp = TempDir::new().unwrap();
    let skill_dir = tmp.path().join("skills");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("deploy.md"),
        "---\nname: deploy\ndescription: Deploy\nagent: devops\n---\n\n# Deploy\nDeploy steps.\n",
    )
    .unwrap();
    let loader = create_test_loader(Some(&skill_dir));
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("deploy"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert_eq!(result.metadata.get("skill_agent").unwrap(), "devops");
}

#[tokio::test]
async fn test_invoke_skill_without_agent_no_metadata() {
    let loader = create_test_loader(None);
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("commit"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.metadata.get("skill_agent").is_none());
}

#[tokio::test]
async fn test_skill_output_wrapped_in_xml() {
    let loader = create_test_loader(None);
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("commit"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("<skill_content name=\"commit\">"));
    assert!(output.contains("</skill_content>"));
    assert!(result.metadata.get("token_estimate").is_some());
}

#[tokio::test]
async fn test_load_namespaced_skill() {
    let tmp = TempDir::new().unwrap();
    let skill_dir = tmp.path().join("skills");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("rebase.md"),
        "---\nname: rebase\ndescription: Git rebase\nnamespace: git\n---\n\n# Rebase\n",
    )
    .unwrap();
    let loader = create_test_loader(Some(&skill_dir));
    let tool = InvokeSkillTool::new(loader);
    let ctx = ToolContext::new("/tmp/test");
    let mut args = HashMap::new();
    args.insert("skill_name".to_string(), serde_json::json!("git:rebase"));
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("Loaded skill: rebase"));
    assert_eq!(result.metadata.get("skill_namespace").unwrap(), "git");
}
