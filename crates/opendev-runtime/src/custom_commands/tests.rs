use super::*;
use tempfile::TempDir;

#[test]
fn test_loader_empty_dir() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join(".opendev").join("commands");
    fs::create_dir_all(&cmd_dir).unwrap();

    let mut loader = CustomCommandLoader::new(tmp.path());
    let commands = loader.load_commands();
    assert!(commands.is_empty());
}

#[test]
fn test_loader_loads_md_files() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join(".opendev").join("commands");
    fs::create_dir_all(&cmd_dir).unwrap();
    fs::write(
        cmd_dir.join("review.md"),
        "# Code review\nReview $ARGUMENTS",
    )
    .unwrap();
    fs::write(cmd_dir.join("_hidden.md"), "should be skipped").unwrap();
    fs::write(cmd_dir.join(".secret.txt"), "should be skipped").unwrap();

    let mut loader = CustomCommandLoader::new(tmp.path());
    let commands = loader.load_commands();
    assert_eq!(commands.len(), 1);
    let review = &commands["review"];
    assert_eq!(review.name, "review");
    assert_eq!(review.description, "Code review");
    assert!(review.source.contains("project:review.md"));
}

#[test]
fn test_loader_caching_and_reload() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join(".opendev").join("commands");
    fs::create_dir_all(&cmd_dir).unwrap();
    fs::write(cmd_dir.join("hello.txt"), "Hello $ARGUMENTS").unwrap();

    let mut loader = CustomCommandLoader::new(tmp.path());
    assert_eq!(loader.load_commands().len(), 1);

    // Add another file — should still be cached
    fs::write(cmd_dir.join("bye.txt"), "Bye $ARGUMENTS").unwrap();
    assert_eq!(loader.load_commands().len(), 1);

    // After reload, picks up new file
    loader.reload();
    assert_eq!(loader.load_commands().len(), 2);
}

#[test]
fn test_list_and_get_commands() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join(".opendev").join("commands");
    fs::create_dir_all(&cmd_dir).unwrap();
    fs::write(cmd_dir.join("greet"), "# Greet someone\nHi $1!").unwrap();

    let mut loader = CustomCommandLoader::new(tmp.path());
    let list = loader.list_commands();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "greet");

    let cmd = loader.get_command("greet").unwrap();
    assert_eq!(cmd.expand("World", None), "# Greet someone\nHi World!");

    assert!(loader.get_command("nonexistent").is_none());
}

#[test]
fn test_loader_frontmatter_metadata() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join(".opendev").join("commands");
    fs::create_dir_all(&cmd_dir).unwrap();
    fs::write(
        cmd_dir.join("commit.md"),
        "---\ndescription: Git commit\nmodel: gpt-4o\nagent: committer\nsubtask: true\n---\n\nCommit $ARGUMENTS",
    )
    .unwrap();

    let mut loader = CustomCommandLoader::new(tmp.path());
    let cmd = loader.get_command("commit").unwrap();
    assert_eq!(cmd.description, "Git commit");
    assert_eq!(cmd.model.as_deref(), Some("gpt-4o"));
    assert_eq!(cmd.agent.as_deref(), Some("committer"));
    assert!(cmd.subtask);
    // Template should not contain frontmatter
    assert!(!cmd.template.contains("---"));
    assert!(cmd.template.contains("Commit $ARGUMENTS"));
}

#[test]
fn test_frontmatter_description_overrides_hash() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join(".opendev").join("commands");
    fs::create_dir_all(&cmd_dir).unwrap();
    fs::write(
        cmd_dir.join("test.md"),
        "---\ndescription: From frontmatter\n---\n# From hash line\nBody",
    )
    .unwrap();

    let mut loader = CustomCommandLoader::new(tmp.path());
    let cmd = loader.get_command("test").unwrap();
    assert_eq!(cmd.description, "From frontmatter");
}

// ── Only .opendev/commands/ is supported ──

#[test]
fn test_loader_claude_commands_dir_not_loaded() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join(".claude").join("commands");
    fs::create_dir_all(&cmd_dir).unwrap();
    fs::write(cmd_dir.join("deploy.md"), "# Deploy\nDeploy $1").unwrap();

    let mut loader = CustomCommandLoader::new(tmp.path());
    let commands = loader.load_commands();
    assert_eq!(commands.len(), 0);
}

#[test]
fn test_command_info_includes_model_agent() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join(".opendev").join("commands");
    fs::create_dir_all(&cmd_dir).unwrap();
    fs::write(
        cmd_dir.join("smart.md"),
        "---\ndescription: Smart cmd\nmodel: claude-opus\nagent: researcher\n---\nDo $ARGUMENTS",
    )
    .unwrap();

    let mut loader = CustomCommandLoader::new(tmp.path());
    let list = loader.list_commands();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].model.as_deref(), Some("claude-opus"));
    assert_eq!(list[0].agent.as_deref(), Some("researcher"));
}
