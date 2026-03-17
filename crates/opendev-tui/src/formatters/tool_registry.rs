//! Centralized tool display registry — single source of truth for how tools appear in the TUI.
//!
//! Replaces the scattered match blocks in the old `tool_colors.rs` with a static registry.
//! Adding a new tool = adding ONE entry to `TOOL_REGISTRY`.

use ratatui::style::Color;
use std::collections::HashMap;

use super::style_tokens;

/// Tool category for grouping purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    /// File read operations (read_file, read_pdf, list_files).
    FileRead,
    /// File write/edit operations (write_file, edit_file).
    FileWrite,
    /// Bash/command execution.
    Bash,
    /// Search operations (search, web_search).
    Search,
    /// Web operations (fetch_url, open_browser, screenshots).
    Web,
    /// Subagent/agent spawn operations.
    Agent,
    /// Symbol/LSP operations (find_symbol, rename_symbol).
    Symbol,
    /// MCP tool calls.
    Mcp,
    /// Plan/task management tools.
    Plan,
    /// Docker operations.
    Docker,
    /// User interaction (ask_user).
    UserInteraction,
    /// Notebook operations.
    Notebook,
    /// Unknown/other tools.
    Other,
}

/// Which result formatter to use for a tool's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultFormat {
    Bash,
    File,
    Directory,
    Generic,
    Todo,
}

/// Single source of truth for how a tool appears in the TUI.
pub struct ToolDisplayEntry {
    /// Tool name(s) this entry matches (exact match).
    pub names: &'static [&'static str],
    /// Category for grouping.
    pub category: ToolCategory,
    /// Display verb shown in TUI, e.g. "Read", "Bash".
    pub verb: &'static str,
    /// Fallback noun when no arg is available, e.g. "file", "command".
    pub label: &'static str,
    /// Ordered keys to try when extracting the primary arg for display.
    pub primary_arg_keys: &'static [&'static str],
    /// Which result formatter to use.
    pub result_format: ResultFormat,
}

/// The static registry — single source of truth for all tool display metadata.
static TOOL_REGISTRY: &[ToolDisplayEntry] = &[
    // File read tools
    ToolDisplayEntry {
        names: &["read_file", "Read"],
        category: ToolCategory::FileRead,
        verb: "Read",
        label: "file",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    ToolDisplayEntry {
        names: &["read_pdf"],
        category: ToolCategory::FileRead,
        verb: "Read",
        label: "pdf",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    ToolDisplayEntry {
        names: &["list_files", "Glob"],
        category: ToolCategory::FileRead,
        verb: "List",
        label: "files",
        primary_arg_keys: &["path", "directory", "pattern"],
        result_format: ResultFormat::Directory,
    },
    // File write tools
    ToolDisplayEntry {
        names: &["write_file", "Write"],
        category: ToolCategory::FileWrite,
        verb: "Write",
        label: "file",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    ToolDisplayEntry {
        names: &["edit_file", "Edit"],
        category: ToolCategory::FileWrite,
        verb: "Edit",
        label: "file",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    ToolDisplayEntry {
        names: &["multi_edit"],
        category: ToolCategory::FileWrite,
        verb: "Edit",
        label: "file",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    ToolDisplayEntry {
        names: &["patch_file"],
        category: ToolCategory::FileWrite,
        verb: "Patch",
        label: "file",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    // Bash/command tools
    ToolDisplayEntry {
        names: &["run_command", "bash_execute", "Bash"],
        category: ToolCategory::Bash,
        verb: "Bash",
        label: "command",
        primary_arg_keys: &["command"],
        result_format: ResultFormat::Bash,
    },
    // Search tools
    ToolDisplayEntry {
        names: &["search", "Grep"],
        category: ToolCategory::Search,
        verb: "Search",
        label: "project",
        primary_arg_keys: &["pattern", "query"],
        result_format: ResultFormat::Directory,
    },
    ToolDisplayEntry {
        names: &["web_search"],
        category: ToolCategory::Search,
        verb: "Search",
        label: "web",
        primary_arg_keys: &["query", "pattern"],
        result_format: ResultFormat::Generic,
    },
    // Web tools
    ToolDisplayEntry {
        names: &["fetch_url"],
        category: ToolCategory::Web,
        verb: "Fetch",
        label: "url",
        primary_arg_keys: &["url"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["open_browser"],
        category: ToolCategory::Web,
        verb: "Open",
        label: "browser",
        primary_arg_keys: &["url"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["capture_screenshot"],
        category: ToolCategory::Web,
        verb: "Capture Screenshot",
        label: "screenshot",
        primary_arg_keys: &["url", "path"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["capture_web_screenshot"],
        category: ToolCategory::Web,
        verb: "Capture Web Screenshot",
        label: "page",
        primary_arg_keys: &["url"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["analyze_image"],
        category: ToolCategory::Web,
        verb: "Analyze Image",
        label: "image",
        primary_arg_keys: &["path", "url"],
        result_format: ResultFormat::Generic,
    },
    // Agent tools
    ToolDisplayEntry {
        names: &["spawn_subagent"],
        category: ToolCategory::Agent,
        verb: "Spawn",
        label: "subagent",
        primary_arg_keys: &["description"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["get_subagent_output"],
        category: ToolCategory::Agent,
        verb: "Get Output",
        label: "subagent",
        primary_arg_keys: &["subagent_id", "id"],
        result_format: ResultFormat::Generic,
    },
    // Symbol tools
    ToolDisplayEntry {
        names: &["find_symbol"],
        category: ToolCategory::Symbol,
        verb: "Find Symbol",
        label: "symbol",
        primary_arg_keys: &["name", "symbol"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["find_referencing_symbols"],
        category: ToolCategory::Symbol,
        verb: "Find References",
        label: "symbol",
        primary_arg_keys: &["name", "symbol"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["insert_before_symbol"],
        category: ToolCategory::Symbol,
        verb: "Insert Before",
        label: "symbol",
        primary_arg_keys: &["name", "symbol"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["insert_after_symbol"],
        category: ToolCategory::Symbol,
        verb: "Insert After",
        label: "symbol",
        primary_arg_keys: &["name", "symbol"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["replace_symbol_body"],
        category: ToolCategory::Symbol,
        verb: "Replace Symbol",
        label: "symbol",
        primary_arg_keys: &["name", "symbol"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["rename_symbol"],
        category: ToolCategory::Symbol,
        verb: "Rename Symbol",
        label: "symbol",
        primary_arg_keys: &["name", "symbol"],
        result_format: ResultFormat::Generic,
    },
    // Plan/task tools
    ToolDisplayEntry {
        names: &["present_plan"],
        category: ToolCategory::Plan,
        verb: "Present Plan",
        label: "plan",
        primary_arg_keys: &["name", "title"],
        result_format: ResultFormat::Generic,
    },
    ToolDisplayEntry {
        names: &["write_todos"],
        category: ToolCategory::Plan,
        verb: "Todos",
        label: "todos",
        primary_arg_keys: &["name", "title"],
        result_format: ResultFormat::Todo,
    },
    ToolDisplayEntry {
        names: &["update_todo"],
        category: ToolCategory::Plan,
        verb: "Update Todo",
        label: "todo",
        primary_arg_keys: &["id", "name"],
        result_format: ResultFormat::Todo,
    },
    ToolDisplayEntry {
        names: &["complete_todo"],
        category: ToolCategory::Plan,
        verb: "Complete Todo",
        label: "todo",
        primary_arg_keys: &["id", "name"],
        result_format: ResultFormat::Todo,
    },
    ToolDisplayEntry {
        names: &["list_todos"],
        category: ToolCategory::Plan,
        verb: "List Todos",
        label: "todos",
        primary_arg_keys: &[],
        result_format: ResultFormat::Todo,
    },
    ToolDisplayEntry {
        names: &["clear_todos"],
        category: ToolCategory::Plan,
        verb: "Clear Todos",
        label: "todos",
        primary_arg_keys: &[],
        result_format: ResultFormat::Todo,
    },
    ToolDisplayEntry {
        names: &["task_complete"],
        category: ToolCategory::Plan,
        verb: "Complete",
        label: "task",
        primary_arg_keys: &["status", "message"],
        result_format: ResultFormat::Generic,
    },
    // User interaction
    ToolDisplayEntry {
        names: &["ask_user"],
        category: ToolCategory::UserInteraction,
        verb: "Ask",
        label: "user",
        primary_arg_keys: &["question", "message"],
        result_format: ResultFormat::Generic,
    },
    // Notebook
    ToolDisplayEntry {
        names: &["notebook_edit"],
        category: ToolCategory::Notebook,
        verb: "Edit",
        label: "notebook",
        primary_arg_keys: &["path", "file_path"],
        result_format: ResultFormat::File,
    },
    // Misc
    ToolDisplayEntry {
        names: &["invoke_skill"],
        category: ToolCategory::Other,
        verb: "Skill",
        label: "skill",
        primary_arg_keys: &["name", "skill"],
        result_format: ResultFormat::Generic,
    },
];

/// Default entry for unknown tools.
static DEFAULT_ENTRY: ToolDisplayEntry = ToolDisplayEntry {
    names: &[],
    category: ToolCategory::Other,
    verb: "Call",
    label: "tool",
    primary_arg_keys: &[
        "command",
        "file_path",
        "path",
        "url",
        "query",
        "pattern",
        "name",
    ],
    result_format: ResultFormat::Generic,
};

/// MCP fallback entry.
static MCP_ENTRY: ToolDisplayEntry = ToolDisplayEntry {
    names: &[],
    category: ToolCategory::Mcp,
    verb: "MCP",
    label: "tool",
    primary_arg_keys: &[
        "command",
        "file_path",
        "path",
        "url",
        "query",
        "pattern",
        "name",
    ],
    result_format: ResultFormat::Generic,
};

/// Docker fallback entry.
static DOCKER_ENTRY: ToolDisplayEntry = ToolDisplayEntry {
    names: &[],
    category: ToolCategory::Docker,
    verb: "Docker",
    label: "operation",
    primary_arg_keys: &["command", "container", "image", "name"],
    result_format: ResultFormat::Generic,
};

/// Look up a tool's display metadata by name.
///
/// Exact match first, then prefix fallback for `mcp__*` and `docker_*`.
pub fn lookup_tool(name: &str) -> &'static ToolDisplayEntry {
    // Exact match in registry
    for entry in TOOL_REGISTRY {
        if entry.names.contains(&name) {
            return entry;
        }
    }

    // Prefix fallbacks
    if name.starts_with("mcp__") {
        return &MCP_ENTRY;
    }
    if name.starts_with("docker_") {
        return &DOCKER_ENTRY;
    }

    &DEFAULT_ENTRY
}

/// Classify a tool name into its category.
pub fn categorize_tool(tool_name: &str) -> ToolCategory {
    lookup_tool(tool_name).category
}

/// Get the primary display color for a tool category.
///
/// All tools use orange (WARNING) for unified appearance.
pub fn tool_color(_category: ToolCategory) -> Color {
    style_tokens::WARNING
}

/// Human-friendly display name for a tool.
///
/// Returns `(verb, label)`.
pub fn tool_display_parts(tool_name: &str) -> (&'static str, &'static str) {
    let entry = lookup_tool(tool_name);
    (entry.verb, entry.label)
}

/// Extract a meaningful argument summary from args using the given keys.
fn extract_arg_from_keys(
    keys: &[&str],
    args: &HashMap<String, serde_json::Value>,
) -> Option<String> {
    if args.is_empty() {
        return None;
    }

    for key in keys {
        if let Some(val) = args.get(*key)
            && let Some(s) = val.as_str()
        {
            let display = s.replace('\n', " ");
            if display.len() > 80 {
                return Some(format!("{}...", &display[..77]));
            }
            return Some(display);
        }
    }

    None
}

/// Format a tool call with arguments for display.
///
/// Returns a string like `Read(/path/to/file.rs)` or `Bash(ls -la)`.
pub fn format_tool_call_display(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
) -> String {
    let (verb, arg) = format_tool_call_parts(tool_name, args);
    format!("{verb}({arg})")
}

/// Strip the working directory prefix from a path to show relative paths.
///
/// E.g., `/Users/me/project/src/main.rs` with working_dir `/Users/me/project` → `src/main.rs`
fn make_relative(path: &str, working_dir: Option<&str>) -> String {
    if let Some(wd) = working_dir
        && !wd.is_empty()
        && path.starts_with(wd)
    {
        let rel = path.strip_prefix(wd).unwrap_or(path);
        let rel = rel.strip_prefix('/').unwrap_or(rel);
        if !rel.is_empty() {
            return rel.to_string();
        }
    }
    // Strip leading "./" for cleaner display — LLMs sometimes produce these
    let cleaned = path.strip_prefix("./").unwrap_or(path);
    cleaned.to_string()
}

/// Format a tool call into separate verb and arg parts.
///
/// Returns `(verb, arg_summary)` — e.g. `("Read", "src/main.rs")` or `("Bash", "ls -la")`.
/// If `working_dir` is provided, file paths are shown relative to it.
pub fn format_tool_call_parts(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
) -> (String, String) {
    format_tool_call_parts_with_wd(tool_name, args, None)
}

/// Format a tool call into separate verb and arg parts, with optional working directory
/// for displaying relative paths.
pub fn format_tool_call_parts_with_wd(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
    working_dir: Option<&str>,
) -> (String, String) {
    let entry = lookup_tool(tool_name);

    // Special case: spawn_subagent shows "AgentType(task_summary)" instead of "Spawn(subagent)"
    if tool_name == "spawn_subagent" {
        let verb = args
            .get("agent_type")
            .and_then(|v| v.as_str())
            .map(|s| {
                // Prettify agent_type names for display
                match s {
                    "Explore" | "Code-Explorer" | "code_explorer" => "Explore".to_string(),
                    "Planner" | "planner" => "Plan".to_string(),
                    "ask-user" | "ask_user" => "AskUser".to_string(),
                    other => other.to_string(),
                }
            })
            .unwrap_or_else(|| "Agent".to_string());
        let task =
            extract_arg_from_keys(&["task"], args).unwrap_or_else(|| "working...".to_string());
        return (verb, task);
    }

    // Special case: search tools show "pattern" in path
    if matches!(tool_name, "search" | "Grep") {
        let pattern = args
            .get("pattern")
            .or_else(|| args.get("query"))
            .and_then(|v| v.as_str())
            .unwrap_or("...");
        let pattern_display = if pattern.len() > 40 {
            format!("\"{}...\"", &pattern[..37])
        } else {
            format!("\"{pattern}\"")
        };
        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
            let rel = make_relative(path, working_dir);
            return ("Search".to_string(), format!("{pattern_display} in {rel}"));
        }
        return ("Search".to_string(), pattern_display);
    }

    // Try to extract a meaningful summary from args
    if let Some(summary) = extract_arg_from_keys(entry.primary_arg_keys, args) {
        // Strip working dir prefix from file path args
        let is_path_arg = entry
            .primary_arg_keys
            .first()
            .is_some_and(|k| *k == "file_path" || *k == "path");
        let summary = if is_path_arg {
            make_relative(&summary, working_dir)
        } else {
            summary
        };
        return (entry.verb.to_string(), summary);
    }

    // MCP tool: show server/tool format
    if tool_name.starts_with("mcp__") {
        let parts: Vec<&str> = tool_name.splitn(3, "__").collect();
        if parts.len() == 3 {
            return ("MCP".to_string(), format!("{}/{}", parts[1], parts[2]));
        }
    }

    // Fallback: verb(label)
    (entry.verb.to_string(), entry.label.to_string())
}

/// Green gradient colors for nested tool spinner animation.
pub const GREEN_GRADIENT: &[Color] = &[
    Color::Rgb(0, 200, 80),
    Color::Rgb(0, 220, 100),
    Color::Rgb(0, 240, 120),
    Color::Rgb(0, 255, 140),
    Color::Rgb(0, 240, 120),
    Color::Rgb(0, 220, 100),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_duplicate_names_in_registry() {
        let mut seen = std::collections::HashSet::new();
        for entry in TOOL_REGISTRY {
            for name in entry.names {
                assert!(seen.insert(name), "Duplicate tool name in registry: {name}");
            }
        }
    }

    #[test]
    fn test_categorize_tool() {
        assert_eq!(categorize_tool("read_file"), ToolCategory::FileRead);
        assert_eq!(categorize_tool("edit_file"), ToolCategory::FileWrite);
        assert_eq!(categorize_tool("run_command"), ToolCategory::Bash);
        assert_eq!(categorize_tool("mcp__server__func"), ToolCategory::Mcp);
        assert_eq!(categorize_tool("docker_start"), ToolCategory::Docker);
        assert_eq!(categorize_tool("unknown_tool"), ToolCategory::Other);
    }

    #[test]
    fn test_tool_display_parts() {
        assert_eq!(tool_display_parts("read_file"), ("Read", "file"));
        assert_eq!(tool_display_parts("run_command"), ("Bash", "command"));
        assert_eq!(tool_display_parts("mcp__something"), ("MCP", "tool"));
    }

    #[test]
    fn test_format_tool_call_display() {
        let mut args = std::collections::HashMap::new();
        args.insert(
            "command".to_string(),
            serde_json::Value::String("ls -la".to_string()),
        );
        let display = format_tool_call_display("run_command", &args);
        assert_eq!(display, "Bash(ls -la)");
    }

    #[test]
    fn test_format_tool_call_no_args() {
        let args = std::collections::HashMap::new();
        let display = format_tool_call_display("list_todos", &args);
        assert_eq!(display, "List Todos(todos)");
    }

    #[test]
    fn test_format_mcp_tool() {
        let args = std::collections::HashMap::new();
        let display = format_tool_call_display("mcp__sqlite__query", &args);
        assert_eq!(display, "MCP(sqlite/query)");
    }

    #[test]
    fn test_all_tools_have_consistent_color() {
        // All categories should return the same orange color
        let categories = [
            ToolCategory::FileRead,
            ToolCategory::FileWrite,
            ToolCategory::Bash,
            ToolCategory::Search,
            ToolCategory::Web,
            ToolCategory::Agent,
            ToolCategory::Symbol,
            ToolCategory::Mcp,
            ToolCategory::Plan,
            ToolCategory::Docker,
            ToolCategory::UserInteraction,
            ToolCategory::Notebook,
            ToolCategory::Other,
        ];
        for cat in categories {
            assert_eq!(tool_color(cat), style_tokens::WARNING);
        }
    }

    #[test]
    fn test_lookup_tool_exact_match() {
        let entry = lookup_tool("read_file");
        assert_eq!(entry.verb, "Read");
        assert_eq!(entry.label, "file");
        assert_eq!(entry.category, ToolCategory::FileRead);
    }

    #[test]
    fn test_lookup_tool_prefix_fallback() {
        let entry = lookup_tool("mcp__some_server__some_tool");
        assert_eq!(entry.category, ToolCategory::Mcp);
        assert_eq!(entry.verb, "MCP");

        let entry = lookup_tool("docker_run");
        assert_eq!(entry.category, ToolCategory::Docker);
        assert_eq!(entry.verb, "Docker");
    }

    #[test]
    fn test_lookup_tool_unknown() {
        let entry = lookup_tool("completely_unknown");
        assert_eq!(entry.category, ToolCategory::Other);
        assert_eq!(entry.verb, "Call");
        assert_eq!(entry.label, "tool");
    }

    #[test]
    fn test_result_format_mapping() {
        assert_eq!(lookup_tool("run_command").result_format, ResultFormat::Bash);
        assert_eq!(lookup_tool("read_file").result_format, ResultFormat::File);
        assert_eq!(
            lookup_tool("list_files").result_format,
            ResultFormat::Directory
        );
        assert_eq!(lookup_tool("ask_user").result_format, ResultFormat::Generic);
    }
}
