use super::*;
use std::collections::HashMap;

// Access the static registry for tests.
use crate::formatters::tool_entries::TOOL_REGISTRY;

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
    assert_eq!(display, "Bash ls -la");
}

#[test]
fn test_format_tool_call_no_args() {
    let args = std::collections::HashMap::new();
    let display = format_tool_call_display("list_todos", &args);
    assert_eq!(display, "List Todos todos");
}

#[test]
fn test_format_mcp_tool() {
    let args = std::collections::HashMap::new();
    let display = format_tool_call_display("mcp__sqlite__query", &args);
    assert_eq!(display, "MCP sqlite/query");
}

#[test]
fn test_all_tools_have_consistent_color() {
    // All categories should return the same orange color
    use crate::formatters::style_tokens;
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

#[test]
fn test_list_files_shows_pattern() {
    let mut args = HashMap::new();
    args.insert(
        "pattern".to_string(),
        serde_json::json!("packages/*/package.json"),
    );
    args.insert("path".to_string(), serde_json::json!("."));
    let (verb, arg) = format_tool_call_parts("list_files", &args);
    assert_eq!(verb, "List");
    assert_eq!(arg, "packages/*/package.json");
}

#[test]
fn test_list_files_shows_pattern_with_path() {
    let mut args = HashMap::new();
    args.insert("pattern".to_string(), serde_json::json!("**/*.ts"));
    args.insert(
        "path".to_string(),
        serde_json::json!("/Users/me/project/src"),
    );
    let (verb, arg) =
        format_tool_call_parts_with_wd("list_files", &args, Some("/Users/me/project"));
    assert_eq!(verb, "List");
    assert_eq!(arg, "**/*.ts in src");
}

#[test]
fn test_list_files_pattern_only() {
    let mut args = HashMap::new();
    args.insert("pattern".to_string(), serde_json::json!("**/*.rs"));
    let (verb, arg) = format_tool_call_parts("list_files", &args);
    assert_eq!(verb, "List");
    assert_eq!(arg, "**/*.rs");
}

#[test]
fn test_ast_grep_shows_pattern() {
    let mut args = HashMap::new();
    args.insert(
        "pattern".to_string(),
        serde_json::json!("fn $NAME($$$ARGS)"),
    );
    let (verb, arg) = format_tool_call_parts("ast_grep", &args);
    assert_eq!(verb, "AST-Grep");
    assert_eq!(arg, "\"fn $NAME($$$ARGS)\"");
}

#[test]
fn test_ast_grep_shows_pattern_with_lang() {
    let mut args = HashMap::new();
    args.insert(
        "pattern".to_string(),
        serde_json::json!("fn $NAME($$$ARGS)"),
    );
    args.insert("lang".to_string(), serde_json::json!("rust"));
    let (verb, arg) = format_tool_call_parts("ast_grep", &args);
    assert_eq!(verb, "AST-Grep");
    assert_eq!(arg, "\"fn $NAME($$$ARGS)\" [rust]");
}

#[test]
fn test_ast_grep_long_pattern_truncated() {
    let mut args = HashMap::new();
    args.insert(
        "pattern".to_string(),
        serde_json::json!("function $NAME($$$PARAMS): Promise<$RET> { $$$BODY }"),
    );
    let (verb, arg) = format_tool_call_parts("ast_grep", &args);
    assert_eq!(verb, "AST-Grep");
    // Pattern > 40 chars gets truncated with "..."
    assert!(arg.contains("..."), "should truncate long pattern: {arg}");
}
