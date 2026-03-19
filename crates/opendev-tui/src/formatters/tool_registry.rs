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
        names: &["patch_file", "patch"],
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
        names: &["grep", "search", "Grep"],
        category: ToolCategory::Search,
        verb: "Grep",
        label: "project",
        primary_arg_keys: &["pattern", "query"],
        result_format: ResultFormat::Directory,
    },
    ToolDisplayEntry {
        names: &["ast_grep", "AstGrep"],
        category: ToolCategory::Search,
        verb: "AST",
        label: "code",
        primary_arg_keys: &["pattern"],
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
        names: &["fetch_url", "web_fetch"],
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
        names: &["capture_web_screenshot", "web_screenshot"],
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
    // Batch
    ToolDisplayEntry {
        names: &["batch_tool"],
        category: ToolCategory::Other,
        verb: "Batch",
        label: "tools",
        primary_arg_keys: &["invocations"],
        result_format: ResultFormat::Generic,
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
    label: "",
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
            return Some(s.replace('\n', " "));
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

/// Format a tool call into separate verb and arg parts.
///
/// Returns `(verb, arg_summary)` — e.g. `("Read", "src/main.rs")` or `("Bash", "ls -la")`.
/// Uses a default `PathShortener` (home dir only, no working dir).
pub fn format_tool_call_parts(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
) -> (String, String) {
    use super::path_shortener::PathShortener;
    let shortener = PathShortener::default();
    format_tool_call_parts_short(tool_name, args, &shortener)
}

/// Format a tool call into separate verb and arg parts, with optional working directory
/// for displaying relative paths. Convenience wrapper that constructs a temporary
/// `PathShortener` — prefer `format_tool_call_parts_short` with a cached shortener.
pub fn format_tool_call_parts_with_wd(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
    working_dir: Option<&str>,
) -> (String, String) {
    use super::path_shortener::PathShortener;
    let shortener = PathShortener::new(working_dir);
    format_tool_call_parts_short(tool_name, args, &shortener)
}

/// Format a tool call into separate verb and arg parts using a cached `PathShortener`.
///
/// This is the preferred entry point — avoids repeated `dirs::home_dir()` syscalls.
pub fn format_tool_call_parts_short(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
    shortener: &super::path_shortener::PathShortener,
) -> (String, String) {
    let (verb, arg) = format_parts_inner(tool_name, args, shortener);
    let shortened = shortener.shorten_text(&arg);
    let truncated = if shortened.len() > 80 {
        format!("{}...", &shortened[..77])
    } else {
        shortened
    };
    (verb, truncated)
}

/// Inner implementation of tool call formatting (before universal path replacement).
fn format_parts_inner(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
    shortener: &super::path_shortener::PathShortener,
) -> (String, String) {
    let entry = lookup_tool(tool_name);

    // Special case: batch_tool shows tool count
    if tool_name == "batch_tool" {
        let count = args
            .get("invocations")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        return ("Batch".to_string(), format!("{count} tool calls"));
    }

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
        let task = extract_arg_from_keys(&["description", "task"], args)
            .unwrap_or_else(|| "working...".to_string());
        return (verb, task);
    }

    // Special case: grep tools show "pattern" in path
    if matches!(tool_name, "grep" | "search" | "Grep") {
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
            let rel = shortener.shorten(path);
            return ("Grep".to_string(), format!("{pattern_display} in {rel}"));
        }
        return ("Grep".to_string(), pattern_display);
    }

    // Special case: ast_grep tools show "pattern" [lang]
    if matches!(tool_name, "ast_grep" | "AstGrep") {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("...");
        let pattern_display = if pattern.len() > 40 {
            format!("\"{}...\"", &pattern[..37])
        } else {
            format!("\"{pattern}\"")
        };
        if let Some(lang) = args.get("lang").and_then(|v| v.as_str()) {
            return ("AST".to_string(), format!("{pattern_display} [{lang}]"));
        }
        return ("AST".to_string(), pattern_display);
    }

    // Unknown tools: derive pretty display name from tool_name itself
    // e.g. "some_fancy_tool" → "Some Fancy Tool", "git" → "Git"
    // Must be before generic arg extraction so we use the pretty name, not "Call"
    if entry.verb == "Call" {
        let pretty_name = tool_name
            .replace('_', " ")
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(ch) => format!("{}{}", ch.to_uppercase(), c.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        if let Some(arg) = extract_arg_from_keys(entry.primary_arg_keys, args) {
            return (pretty_name, arg);
        }

        return (pretty_name, String::new());
    }

    // Try to extract a meaningful summary from args
    if let Some(summary) = extract_arg_from_keys(entry.primary_arg_keys, args) {
        // Strip working dir prefix from file path args
        let is_path_arg = entry
            .primary_arg_keys
            .first()
            .is_some_and(|k| *k == "file_path" || *k == "path");
        let summary = if is_path_arg {
            shortener.shorten(&summary)
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

    // Known tool with no arg extracted: verb(label)
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
        // Unknown tools still return DEFAULT_ENTRY with empty label
        assert_eq!(tool_display_parts("unknown_xyz"), ("Call", ""));
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
        assert_eq!(entry.label, "");
    }

    #[test]
    fn test_unknown_tool_derives_pretty_name() {
        let args = HashMap::new();
        let (verb, arg) = format_tool_call_parts("some_fancy_tool", &args);
        assert_eq!(verb, "Some Fancy Tool");
        assert_eq!(arg, "");
    }

    #[test]
    fn test_unknown_tool_with_arg() {
        let mut args = HashMap::new();
        args.insert(
            "command".to_string(),
            serde_json::Value::String("do stuff".to_string()),
        );
        let (verb, arg) = format_tool_call_parts("my_tool", &args);
        assert_eq!(verb, "My Tool");
        assert_eq!(arg, "do stuff");
    }

    #[test]
    fn test_batch_tool_display() {
        let mut args = HashMap::new();
        args.insert(
            "invocations".to_string(),
            serde_json::json!([{"tool": "read_file"}, {"tool": "edit_file"}, {"tool": "bash"}]),
        );
        let (verb, arg) = format_tool_call_parts("batch_tool", &args);
        assert_eq!(verb, "Batch");
        assert_eq!(arg, "3 tool calls");
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

    #[test]
    fn test_format_spawn_subagent_strips_paths() {
        let mut args = HashMap::new();
        args.insert(
            "agent_type".to_string(),
            serde_json::Value::String("Explore".to_string()),
        );
        args.insert(
            "task".to_string(),
            serde_json::Value::String(
                "Explore repo at /Users/me/project with focus on tests".to_string(),
            ),
        );
        let (verb, arg) =
            format_tool_call_parts_with_wd("spawn_subagent", &args, Some("/Users/me/project"));
        assert_eq!(verb, "Explore");
        assert_eq!(arg, "Explore repo at . with focus on tests");
    }
}
