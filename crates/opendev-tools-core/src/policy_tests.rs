use super::*;

#[test]
fn test_resolve_full_profile() {
    let allowed = ToolPolicy::resolve("full", None, None).unwrap();
    assert!(allowed.contains("read_file"));
    assert!(allowed.contains("write_file"));
    assert!(allowed.contains("run_command"));
    assert!(allowed.contains("task_complete"));
    assert!(allowed.contains("ask_user"));
    assert!(allowed.contains("send_message"));
    assert!(allowed.contains("schedule"));
}

#[test]
fn test_resolve_minimal_profile() {
    let allowed = ToolPolicy::resolve("minimal", None, None).unwrap();
    assert!(allowed.contains("read_file"));
    assert!(allowed.contains("search"));
    assert!(allowed.contains("task_complete")); // always allowed
    assert!(!allowed.contains("write_file"));
    assert!(!allowed.contains("run_command"));
}

#[test]
fn test_resolve_coding_profile() {
    let allowed = ToolPolicy::resolve("coding", None, None).unwrap();
    assert!(allowed.contains("read_file"));
    assert!(allowed.contains("write_file"));
    assert!(allowed.contains("run_command"));
    assert!(!allowed.contains("send_message")); // not in coding
}

#[test]
fn test_resolve_unknown_profile() {
    let result = ToolPolicy::resolve("nonexistent", None, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown tool profile"));
}

#[test]
fn test_resolve_with_additions() {
    let allowed = ToolPolicy::resolve("minimal", Some(&["custom_tool"]), None).unwrap();
    assert!(allowed.contains("custom_tool"));
    assert!(allowed.contains("read_file"));
}

#[test]
fn test_resolve_with_exclusions() {
    let allowed = ToolPolicy::resolve("full", None, Some(&["run_command"])).unwrap();
    assert!(!allowed.contains("run_command"));
    assert!(allowed.contains("read_file"));
}

#[test]
fn test_resolve_exclusion_overrides_always_allowed() {
    let allowed = ToolPolicy::resolve("minimal", None, Some(&["task_complete"])).unwrap();
    assert!(!allowed.contains("task_complete"));
}

#[test]
fn test_get_profile_names() {
    let names = ToolPolicy::get_profile_names();
    assert!(names.contains(&"minimal"));
    assert!(names.contains(&"full"));
    assert!(names.contains(&"coding"));
    assert!(names.contains(&"review"));
}

#[test]
fn test_get_group_names() {
    let names = ToolPolicy::get_group_names();
    assert!(names.contains(&"group:read"));
    assert!(names.contains(&"group:write"));
    assert!(names.contains(&"group:process"));
}

#[test]
fn test_get_tools_in_group() {
    let tools = ToolPolicy::get_tools_in_group("group:read");
    assert!(tools.contains("read_file"));
    assert!(tools.contains("search"));
    assert!(!tools.contains("write_file"));
}

#[test]
fn test_get_tools_in_unknown_group() {
    let tools = ToolPolicy::get_tools_in_group("group:nonexistent");
    assert!(tools.is_empty());
}

#[test]
fn test_profile_descriptions() {
    assert_eq!(
        ToolPolicy::get_profile_description("minimal"),
        "Read-only tools + meta tools (for planning/exploration)"
    );
    assert_eq!(
        ToolPolicy::get_profile_description("full"),
        "All available tools (default)"
    );
    assert_eq!(
        ToolPolicy::get_profile_description("unknown"),
        "Unknown profile"
    );
}

#[test]
fn test_always_allowed_in_all_profiles() {
    for profile in &["minimal", "review", "coding", "full"] {
        let allowed = ToolPolicy::resolve(profile, None, None).unwrap();
        assert!(
            allowed.contains("task_complete"),
            "task_complete missing from {profile}"
        );
        assert!(
            allowed.contains("ask_user"),
            "ask_user missing from {profile}"
        );
    }
}
