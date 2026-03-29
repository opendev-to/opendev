use super::*;

fn make_command(template: &str) -> CustomCommand {
    CustomCommand {
        name: "test".to_string(),
        template: template.to_string(),
        source: "test:test.md".to_string(),
        description: "Test command".to_string(),
        model: None,
        agent: None,
        subtask: false,
    }
}

// ── Expansion tests ──

#[test]
fn test_expand_arguments() {
    let cmd = make_command("Review this: $ARGUMENTS\nDone.");
    let result = cmd.expand("auth module", None);
    assert_eq!(result, "Review this: auth module\nDone.");
}

#[test]
fn test_expand_positional_args() {
    let cmd = make_command("Fix $1 in $2");
    let result = cmd.expand("bug main.rs", None);
    assert_eq!(result, "Fix bug in main.rs");
}

#[test]
fn test_expand_unreplaced_positional_cleaned() {
    let cmd = make_command("Use $1 and $2 and $3");
    let result = cmd.expand("foo bar", None);
    assert_eq!(result, "Use foo and bar and");
}

#[test]
fn test_expand_empty_arguments() {
    let cmd = make_command("Hello $ARGUMENTS world");
    let result = cmd.expand("", None);
    assert_eq!(result, "Hello  world");
}

#[test]
fn test_expand_context_variables() {
    let cmd = make_command("Check $FILE for $LANG issues");
    let mut ctx = HashMap::new();
    ctx.insert("file".to_string(), "main.rs".to_string());
    ctx.insert("lang".to_string(), "Rust".to_string());
    let result = cmd.expand("", Some(&ctx));
    assert_eq!(result, "Check main.rs for Rust issues");
}

#[test]
fn test_expand_combined() {
    let cmd = make_command("Review $ARGUMENTS in $FILE focusing on $1");
    let mut ctx = HashMap::new();
    ctx.insert("file".to_string(), "lib.rs".to_string());
    let result = cmd.expand("security perf", Some(&ctx));
    assert_eq!(
        result,
        "Review security perf in lib.rs focusing on security"
    );
}

// ── Shell substitution tests ──

#[test]
fn test_expand_shell_command() {
    let cmd = make_command("Hello !`echo world`");
    let result = cmd.expand("", None);
    assert_eq!(result, "Hello world");
}

#[test]
fn test_expand_shell_command_with_args() {
    let cmd = make_command("Version: !`echo 1.2.3`");
    let result = cmd.expand("", None);
    assert_eq!(result, "Version: 1.2.3");
}

#[test]
fn test_expand_shell_command_failure() {
    let cmd = make_command("Result: !`false`");
    let result = cmd.expand("", None);
    assert!(result.starts_with("Result: [error:"));
}

#[test]
fn test_expand_shell_no_substitution() {
    let cmd = make_command("No shell here");
    let result = cmd.expand("", None);
    assert_eq!(result, "No shell here");
}

// ── Frontmatter parsing tests ──

#[test]
fn test_parse_frontmatter_basic() {
    let content = "---\ndescription: Code review\nmodel: gpt-4o\n---\n\nReview $ARGUMENTS";
    let (fm, body) = parse_frontmatter(content);
    assert_eq!(fm.get("description").unwrap(), "Code review");
    assert_eq!(fm.get("model").unwrap(), "gpt-4o");
    assert_eq!(body.trim(), "Review $ARGUMENTS");
}

#[test]
fn test_parse_frontmatter_quoted_values() {
    let content = "---\ndescription: \"Commit and push\"\nagent: 'reviewer'\n---\nBody";
    let (fm, body) = parse_frontmatter(content);
    assert_eq!(fm.get("description").unwrap(), "Commit and push");
    assert_eq!(fm.get("agent").unwrap(), "reviewer");
    assert_eq!(body.trim(), "Body");
}

#[test]
fn test_parse_frontmatter_none() {
    let content = "# No frontmatter\nJust a template";
    let (fm, body) = parse_frontmatter(content);
    assert!(fm.is_empty());
    assert_eq!(body, content);
}

#[test]
fn test_parse_frontmatter_no_closing() {
    let content = "---\nkey: value\nno closing delimiter";
    let (fm, body) = parse_frontmatter(content);
    assert!(fm.is_empty());
    assert_eq!(body, content);
}
