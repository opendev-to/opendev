//! Tool access profiles and group-based permissions.
//!
//! Defines tool groups (read, write, process, etc.) and profiles (minimal, review,
//! coding, full) that compose groups into permission sets.

use std::collections::{HashMap, HashSet};

/// Tool groups — categorize tools by function.
fn tool_groups() -> HashMap<&'static str, HashSet<&'static str>> {
    let mut groups = HashMap::new();

    groups.insert(
        "group:read",
        HashSet::from([
            "read_file",
            "list_files",
            "search",
            "find_symbol",
            "find_referencing_symbols",
            "read_pdf",
            "analyze_image",
        ]),
    );

    groups.insert(
        "group:write",
        HashSet::from([
            "write_file",
            "edit_file",
            "insert_before_symbol",
            "insert_after_symbol",
            "replace_symbol_body",
            "rename_symbol",
            "notebook_edit",
            "apply_patch",
        ]),
    );

    groups.insert("group:process", HashSet::from(["run_command"]));

    groups.insert(
        "group:web",
        HashSet::from([
            "fetch_url",
            "web_search",
            "capture_web_screenshot",
            "capture_screenshot",
            "browser",
            "open_browser",
        ]),
    );

    groups.insert("group:git", HashSet::from(["git"]));

    groups.insert(
        "group:session",
        HashSet::from([
            "list_sessions",
            "get_session_history",
            "spawn_subagent",
            "get_subagent_output",
            "list_subagents",
        ]),
    );

    groups.insert(
        "group:memory",
        HashSet::from(["memory_search", "memory_write"]),
    );

    groups.insert(
        "group:meta",
        HashSet::from([
            "task_complete",
            "ask_user",
            "present_plan",
            "write_todos",
            "update_todo",
            "complete_todo",
            "list_todos",
            "clear_todos",
            "search_tools",
            "invoke_skill",
            "batch_tool",
        ]),
    );

    groups.insert("group:messaging", HashSet::from(["send_message"]));
    groups.insert("group:automation", HashSet::from(["schedule"]));
    groups.insert("group:thinking", HashSet::new());
    groups.insert("group:mcp", HashSet::new());

    groups
}

/// Named profiles — compose groups into permission sets.
fn profiles() -> HashMap<&'static str, Vec<&'static str>> {
    let mut p = HashMap::new();
    p.insert("minimal", vec!["group:read", "group:meta"]);
    p.insert(
        "review",
        vec![
            "group:read",
            "group:meta",
            "group:web",
            "group:git",
            "group:session",
        ],
    );
    p.insert(
        "coding",
        vec![
            "group:read",
            "group:write",
            "group:process",
            "group:web",
            "group:git",
            "group:meta",
            "group:session",
            "group:memory",
        ],
    );
    p.insert(
        "full",
        vec![
            "group:read",
            "group:write",
            "group:process",
            "group:web",
            "group:git",
            "group:session",
            "group:memory",
            "group:meta",
            "group:messaging",
            "group:automation",
            "group:thinking",
            "group:mcp",
        ],
    );
    p
}

/// Tools that are always allowed regardless of profile.
const ALWAYS_ALLOWED: &[&str] = &["task_complete", "ask_user"];

/// Resolves which tools are allowed based on profile, additions, and exclusions.
pub struct ToolPolicy;

impl ToolPolicy {
    /// Resolve the set of allowed tool names for a given profile.
    ///
    /// Returns an error if the profile name is unknown.
    pub fn resolve(
        profile: &str,
        additions: Option<&[&str]>,
        exclusions: Option<&[&str]>,
    ) -> Result<HashSet<String>, String> {
        let all_profiles = profiles();
        let group_names = match all_profiles.get(profile) {
            Some(g) => g,
            None => {
                let available: Vec<_> = all_profiles.keys().collect();
                return Err(format!(
                    "Unknown tool profile: '{}'. Available: {:?}",
                    profile, available
                ));
            }
        };

        let groups = tool_groups();
        let mut allowed: HashSet<String> = HashSet::new();

        // Expand groups into tool names
        for group_name in group_names {
            if let Some(tools) = groups.get(group_name) {
                for tool in tools {
                    allowed.insert((*tool).to_string());
                }
            }
        }

        // Always-allowed tools
        for tool in ALWAYS_ALLOWED {
            allowed.insert((*tool).to_string());
        }

        // Apply additions
        if let Some(adds) = additions {
            for tool in adds {
                allowed.insert((*tool).to_string());
            }
        }

        // Apply exclusions
        if let Some(excls) = exclusions {
            for tool in excls {
                allowed.remove(*tool);
            }
        }

        Ok(allowed)
    }

    /// Return available profile names.
    pub fn get_profile_names() -> Vec<&'static str> {
        let p = profiles();
        let mut names: Vec<_> = p.keys().copied().collect();
        names.sort();
        names
    }

    /// Return available group names.
    pub fn get_group_names() -> Vec<&'static str> {
        let g = tool_groups();
        let mut names: Vec<_> = g.keys().copied().collect();
        names.sort();
        names
    }

    /// Get tool names in a specific group.
    pub fn get_tools_in_group(group_name: &str) -> HashSet<String> {
        let groups = tool_groups();
        groups
            .get(group_name)
            .map(|tools| tools.iter().map(|t| (*t).to_string()).collect())
            .unwrap_or_default()
    }

    /// Get a human-readable description of a profile.
    pub fn get_profile_description(profile: &str) -> &'static str {
        match profile {
            "minimal" => "Read-only tools + meta tools (for planning/exploration)",
            "review" => "Read + web + git + session tools (for code review)",
            "coding" => "Full development toolset without messaging/automation",
            "full" => "All available tools (default)",
            _ => "Unknown profile",
        }
    }
}

#[cfg(test)]
mod tests {
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
        assert!(allowed.contains("git"));
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
}
