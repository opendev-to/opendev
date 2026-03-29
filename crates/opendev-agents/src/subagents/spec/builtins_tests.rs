use super::*;

#[test]
fn test_code_explorer_builtin() {
    let spec = code_explorer("You explore code.");
    assert_eq!(spec.name, "Explore");
    assert!(spec.has_tool_restriction());
    assert!(spec.tools.contains(&"read_file".to_string()));
    assert!(spec.tools.contains(&"grep".to_string()));
    assert!(spec.tools.contains(&"ast_grep".to_string()));
    assert!(!spec.tools.contains(&"write_file".to_string()));
}

#[test]
fn test_planner_builtin() {
    let spec = planner("You plan tasks.");
    assert_eq!(spec.name, "Planner");
    assert!(spec.tools.contains(&"write_file".to_string()));
    assert!(spec.tools.contains(&"edit_file".to_string()));
}

#[test]
fn test_ask_user_builtin() {
    let spec = ask_user("You ask questions.");
    assert_eq!(spec.name, "ask-user");
    assert!(!spec.has_tool_restriction()); // No tools
}

#[test]
fn test_general_builtin() {
    let spec = general("You are versatile.");
    assert_eq!(spec.name, "General");
    assert!(spec.has_tool_restriction());
    // General has broad tool access
    assert!(spec.tools.contains(&"read_file".to_string()));
    assert!(spec.tools.contains(&"write_file".to_string()));
    assert!(spec.tools.contains(&"edit_file".to_string()));
    assert!(spec.tools.contains(&"run_command".to_string()));
    assert!(spec.tools.contains(&"web_fetch".to_string()));
    assert!(spec.tools.contains(&"git".to_string()));
    assert_eq!(spec.tools.len(), GENERAL_TOOLS.len());
}

#[test]
fn test_build_builtin() {
    let spec = build("You build code.");
    assert_eq!(spec.name, "Build");
    assert!(spec.has_tool_restriction());
    assert!(spec.tools.contains(&"run_command".to_string()));
    assert!(spec.tools.contains(&"edit_file".to_string()));
    assert!(spec.tools.contains(&"read_file".to_string()));
    assert_eq!(spec.tools.len(), BUILD_TOOLS.len());
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
    assert!(spec.tools.contains(&"read_file".to_string()));
    assert!(spec.tools.contains(&"list_files".to_string()));
    assert!(spec.tools.contains(&"grep".to_string()));
    assert!(spec.tools.contains(&"run_command".to_string()));
    assert!(spec.model.is_none());
}
