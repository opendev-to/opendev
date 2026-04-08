use super::*;

#[test]
fn test_parse_frontmatter_with_paths() {
    let content =
        "---\npaths:\n  - \"src/**/*.rs\"\n  - \"tests/**/*.rs\"\n---\nRule content here.";
    let (fm, remaining) = parse_frontmatter(content);
    assert!(fm.is_some());
    let fm = fm.unwrap();
    let paths = fm.paths.unwrap();
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0], "src/**/*.rs");
    assert_eq!(paths[1], "tests/**/*.rs");
    assert_eq!(remaining.trim(), "Rule content here.");
}

#[test]
fn test_parse_frontmatter_no_frontmatter() {
    let content = "Just regular content\nwith multiple lines.";
    let (fm, remaining) = parse_frontmatter(content);
    assert!(fm.is_none());
    assert_eq!(remaining, content);
}

#[test]
fn test_parse_frontmatter_empty() {
    let content = "---\n---\nContent after empty frontmatter.";
    let (fm, remaining) = parse_frontmatter(content);
    assert!(fm.is_some());
    assert!(fm.unwrap().paths.is_none());
    assert_eq!(remaining.trim(), "Content after empty frontmatter.");
}

#[test]
fn test_parse_frontmatter_without_paths() {
    let content = "---\nname: my-rule\ndescription: A rule\n---\nContent.";
    let (fm, remaining) = parse_frontmatter(content);
    assert!(fm.is_some());
    assert!(fm.unwrap().paths.is_none());
    assert_eq!(remaining.trim(), "Content.");
}

#[test]
fn test_parse_frontmatter_no_closing_delimiter() {
    let content = "---\npaths:\n  - \"*.rs\"\nContent without closing delimiter.";
    let (fm, remaining) = parse_frontmatter(content);
    assert!(fm.is_none());
    assert_eq!(remaining, content);
}

#[test]
fn test_strip_html_comments_basic() {
    let content = "Before\n<!-- This is a comment -->\nAfter";
    let result = strip_html_comments(content);
    assert!(result.contains("Before"));
    assert!(result.contains("After"));
    assert!(!result.contains("This is a comment"));
}

#[test]
fn test_strip_html_comments_multiline() {
    let content = "Before\n<!--\nMulti\nline\ncomment\n-->\nAfter";
    let result = strip_html_comments(content);
    assert!(result.contains("Before"));
    assert!(result.contains("After"));
    assert!(!result.contains("Multi"));
}

#[test]
fn test_strip_html_comments_in_code_block_preserved() {
    let content = "Before\n```html\n<!-- This should stay -->\n```\nAfter";
    let result = strip_html_comments(content);
    assert!(result.contains("This should stay"));
    assert!(result.contains("Before"));
    assert!(result.contains("After"));
}

#[test]
fn test_strip_html_comments_no_comments() {
    let content = "Just regular content\nNo comments here.";
    let result = strip_html_comments(content);
    assert_eq!(result, content);
}

#[test]
fn test_strip_html_comments_multiple() {
    let content = "A<!-- one -->B<!-- two -->C";
    let result = strip_html_comments(content);
    assert_eq!(result, "ABC");
}
