use super::*;

#[test]
fn test_code_explorer_builtin() {
    let spec = code_explorer("You explore code.");
    assert_eq!(spec.name, "Explore");
    assert!(spec.has_tool_restriction());
    assert!(spec.tools.contains(&"Read".to_string()));
    assert!(spec.tools.contains(&"Grep".to_string()));
    assert!(spec.tools.contains(&"ast_grep".to_string()));
    assert!(!spec.tools.contains(&"Write".to_string()));
}

#[test]
fn test_planner_builtin() {
    let spec = planner("You plan tasks.");
    assert_eq!(spec.name, "Planner");
    assert!(spec.tools.contains(&"Write".to_string()));
    assert!(spec.tools.contains(&"Edit".to_string()));
}

#[test]
fn test_general_builtin() {
    let spec = general("You are versatile.");
    assert_eq!(spec.name, "General");
    // General inherits all parent tools (no restriction)
    assert!(!spec.has_tool_restriction());
    assert!(spec.tools.is_empty());
}

#[test]
fn test_build_builtin() {
    let spec = build("You build code.");
    assert_eq!(spec.name, "Build");
    assert!(spec.has_tool_restriction());
    assert!(spec.tools.contains(&"Bash".to_string()));
    assert!(spec.tools.contains(&"Edit".to_string()));
    assert!(spec.tools.contains(&"Read".to_string()));
    assert_eq!(spec.tools.len(), BUILD_TOOLS.len());
}

#[test]
fn test_verification_builtin() {
    let spec = verification("You verify changes.");
    assert_eq!(spec.name, "Verification");
    assert!(spec.background);
    assert!(spec.has_tool_restriction());
    assert!(spec.tools.contains(&"Read".to_string()));
    assert!(spec.tools.contains(&"Grep".to_string()));
    assert!(spec.tools.contains(&"Glob".to_string()));
    assert!(spec.tools.contains(&"Bash".to_string()));
    assert_eq!(spec.tools.len(), VERIFICATION_TOOLS.len());
}

#[test]
fn test_project_init_builtin() {
    let spec = project_init("You analyze codebases.");
    assert_eq!(spec.name, "project_init");
    assert_eq!(
        spec.description,
        "Analyze codebase and generate project instructions"
    );
    assert!(spec.has_tool_restriction());
    assert_eq!(spec.tools.len(), 4);
    assert!(spec.tools.contains(&"Read".to_string()));
    assert!(spec.tools.contains(&"Glob".to_string()));
    assert!(spec.tools.contains(&"Grep".to_string()));
    assert!(spec.tools.contains(&"Bash".to_string()));
    assert!(spec.model.is_none());
}
