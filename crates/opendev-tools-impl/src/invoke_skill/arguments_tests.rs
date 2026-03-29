use super::*;

#[test]
fn test_expand_arguments_placeholder() {
    let result = expand_skill_arguments("Run this: $ARGUMENTS", "git push origin main");
    assert_eq!(result, "Run this: git push origin main");
}

#[test]
fn test_expand_positional_args() {
    let result = expand_skill_arguments("Source: $1\nTarget: $2", "src/main.rs tests/test.rs");
    assert_eq!(result, "Source: src/main.rs\nTarget: tests/test.rs");
}

#[test]
fn test_expand_quoted_args() {
    let result = expand_skill_arguments("File: $1\nMessage: $2", "README.md \"hello world\"");
    assert_eq!(result, "File: README.md\nMessage: hello world");
}

#[test]
fn test_expand_no_placeholders_appends() {
    let result = expand_skill_arguments("# My Skill\nDo stuff.", "some args here");
    assert!(result.contains("# My Skill\nDo stuff."));
    assert!(result.contains("## Input\n\nsome args here"));
}

#[test]
fn test_expand_empty_args_no_change() {
    let result = expand_skill_arguments("Content with $ARGUMENTS placeholder.", "");
    assert_eq!(result, "Content with  placeholder.");
}

#[test]
fn test_expand_both_arguments_and_positional() {
    let result = expand_skill_arguments("Full: $ARGUMENTS\nFirst: $1\nSecond: $2", "foo bar");
    assert_eq!(result, "Full: foo bar\nFirst: foo\nSecond: bar");
}

#[test]
fn test_expand_more_placeholders_than_args() {
    let result = expand_skill_arguments("A: $1, B: $2, C: $3", "only-one");
    assert_eq!(result, "A: only-one, B: $2, C: $3");
}

#[test]
fn test_parse_positional_args_basic() {
    assert_eq!(
        parse_positional_args("foo bar baz"),
        vec!["foo", "bar", "baz"]
    );
}

#[test]
fn test_parse_positional_args_quoted() {
    let args = parse_positional_args(r#"hello "multi word" 'single quoted'"#);
    assert_eq!(args, vec!["hello", "multi word", "single quoted"]);
}

#[test]
fn test_parse_positional_args_empty() {
    assert!(parse_positional_args("").is_empty());
}

#[test]
fn test_parse_positional_args_unclosed_quote() {
    let args = parse_positional_args(r#"hello "unclosed world"#);
    assert_eq!(args, vec!["hello", "unclosed world"]);
}
