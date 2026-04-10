//! Tool access profiles and group-based permissions.
//!
//! Defines tool groups (read, write, process, etc.) and profiles (minimal, review,
//! coding, full) that compose groups into permission sets.
//!
//! The legacy `tool_groups()` function is a hardcoded tool→group mapping.
//! `resolve_from_registry()` dynamically builds groups from `BaseTool::category()`,
//! so newly registered tools automatically join the correct group.

use std::collections::{HashMap, HashSet};

use crate::traits::ToolCategory;

/// Tool groups — categorize tools by function.
fn tool_groups() -> HashMap<&'static str, HashSet<&'static str>> {
    let mut groups = HashMap::new();

    groups.insert(
        "group:read",
        HashSet::from([
            "Read",
            "Glob",
            "Grep",
            "find_symbol",
            "find_referencing_symbols",
            "read_pdf",
            "analyze_image",
        ]),
    );

    groups.insert(
        "group:write",
        HashSet::from([
            "Write",
            "Edit",
            "insert_before_symbol",
            "insert_after_symbol",
            "replace_symbol_body",
            "rename_symbol",
            "NotebookEdit",
            "apply_patch",
        ]),
    );

    groups.insert("group:process", HashSet::from(["Bash"]));

    groups.insert(
        "group:web",
        HashSet::from([
            "WebFetch",
            "WebSearch",
            "capture_web_screenshot",
            "capture_screenshot",
            "browser",
            "open_browser",
        ]),
    );

    groups.insert(
        "group:session",
        HashSet::from([
            "list_sessions",
            "get_session_history",
            "Agent",
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
            "TaskStop",
            "AskUserQuestion",
            "EnterPlanMode",
            "TodoWrite",
            "TaskUpdate",
            "complete_todo",
            "TaskList",
            "clear_todos",
            "search_tools",
            "Skill",
        ]),
    );

    groups.insert("group:messaging", HashSet::from(["SendMessage"]));
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
        vec!["group:read", "group:meta", "group:web", "group:session"],
    );
    p.insert(
        "coding",
        vec![
            "group:read",
            "group:write",
            "group:process",
            "group:web",
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
const ALWAYS_ALLOWED: &[&str] = &["TaskStop", "AskUserQuestion"];

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

    /// Resolve allowed tools using `BaseTool::category()` from the registry.
    ///
    /// Builds groups dynamically so newly registered tools automatically
    /// join the correct group. Falls back to `resolve()` semantics for
    /// profile→group mapping.
    pub fn resolve_from_registry(
        profile: &str,
        registry: &crate::registry::ToolRegistry,
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

        // Build category → tool names from the registry
        let tool_categories = registry.build_category_map();
        let mut category_map: HashMap<&str, HashSet<String>> = HashMap::new();
        for (cat, names) in &tool_categories {
            let group_name = Self::category_to_group(*cat);
            category_map
                .entry(group_name)
                .or_default()
                .extend(names.iter().cloned());
        }

        // Also include hardcoded groups for tools not yet migrated
        let hardcoded = tool_groups();

        let mut allowed: HashSet<String> = HashSet::new();

        for group_name in group_names {
            // Prefer dynamic groups; fall back to hardcoded
            if let Some(tools) = category_map.get(group_name) {
                allowed.extend(tools.iter().cloned());
            }
            if let Some(tools) = hardcoded.get(group_name) {
                for tool in tools {
                    allowed.insert((*tool).to_string());
                }
            }
        }

        for tool in ALWAYS_ALLOWED {
            allowed.insert((*tool).to_string());
        }

        if let Some(adds) = additions {
            for tool in adds {
                allowed.insert((*tool).to_string());
            }
        }

        if let Some(excls) = exclusions {
            for tool in excls {
                allowed.remove(*tool);
            }
        }

        Ok(allowed)
    }

    /// Map a `ToolCategory` to the corresponding group name string.
    pub fn category_to_group(category: ToolCategory) -> &'static str {
        match category {
            ToolCategory::Read => "group:read",
            ToolCategory::Write => "group:write",
            ToolCategory::Process => "group:process",
            ToolCategory::Web => "group:web",
            ToolCategory::Session => "group:session",
            ToolCategory::Memory => "group:memory",
            ToolCategory::Meta => "group:meta",
            ToolCategory::Messaging => "group:messaging",
            ToolCategory::Automation => "group:automation",
            ToolCategory::Symbol => "group:read", // Symbol tools are read-friendly
            ToolCategory::Mcp => "group:mcp",
            ToolCategory::Other => "group:meta", // Safe default
        }
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
#[path = "policy_tests.rs"]
mod tests;
