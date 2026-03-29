use super::*;

#[test]
fn test_parse_frontmatter_basic() {
    let content = "---\ndescription: test\n---\nBody here.";
    let (fm, body) = parse_frontmatter(content);
    assert_eq!(fm, Some("description: test"));
    assert_eq!(body, "Body here.");
}

#[test]
fn test_parse_frontmatter_none() {
    let content = "Just a body with no frontmatter.";
    let (fm, body) = parse_frontmatter(content);
    assert!(fm.is_none());
    assert_eq!(body, content);
}

#[test]
fn test_parse_frontmatter_no_closing() {
    let content = "---\ndescription: test\nNo closing delimiter.";
    let (fm, body) = parse_frontmatter(content);
    assert!(fm.is_none());
    assert_eq!(body, content);
}

#[test]
fn test_parse_simple_yaml() {
    let yaml = "description: \"Reviews code\"\ntools:\n  - read_file\n  - search";
    let meta = parse_simple_yaml(yaml);
    assert_eq!(meta.description.as_deref(), Some("Reviews code"));
    assert_eq!(meta.tools, vec!["read_file", "search"]);
}

#[test]
fn test_parse_simple_yaml_disabled() {
    let yaml = "disabled: true\nmodel: gpt-4o";
    let meta = parse_simple_yaml(yaml);
    assert!(meta.disabled);
    assert_eq!(meta.model.as_deref(), Some("gpt-4o"));
}

#[test]
fn test_parse_permission_blanket_action() {
    let yaml = "permission:\n  bash: deny\n  edit: allow";
    let meta = parse_simple_yaml(yaml);
    assert_eq!(meta.permission.len(), 2);
    assert!(matches!(
        meta.permission["bash"],
        PermissionRule::Action(PermissionAction::Deny)
    ));
    assert!(matches!(
        meta.permission["edit"],
        PermissionRule::Action(PermissionAction::Allow)
    ));
}

#[test]
fn test_parse_permission_with_patterns() {
    let yaml =
        "permission:\n  bash:\n    \"*\": ask\n    \"git *\": allow\n    \"rm -rf *\": deny";
    let meta = parse_simple_yaml(yaml);
    assert_eq!(meta.permission.len(), 1);
    if let PermissionRule::Patterns(ref p) = meta.permission["bash"] {
        assert_eq!(p.len(), 3);
        assert_eq!(p["*"], PermissionAction::Ask);
        assert_eq!(p["git *"], PermissionAction::Allow);
        assert_eq!(p["rm -rf *"], PermissionAction::Deny);
    } else {
        panic!("Expected Patterns variant");
    }
}

#[test]
fn test_parse_permission_mixed() {
    let yaml = "permission:\n  edit: deny\n  bash:\n    \"*\": ask\n    \"git *\": allow";
    let meta = parse_simple_yaml(yaml);
    assert_eq!(meta.permission.len(), 2);
    assert!(matches!(
        meta.permission["edit"],
        PermissionRule::Action(PermissionAction::Deny)
    ));
    assert!(matches!(
        meta.permission["bash"],
        PermissionRule::Patterns(_)
    ));
}
