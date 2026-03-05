"""Tool metadata for agent creation wizard.

Provides a centralized source for tool information including display names
and descriptions for the tool selection UI.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import List


@dataclass
class ToolInfo:
    """Metadata about a tool for the selection UI."""

    name: str  # Internal name (e.g., "read_file")
    display_name: str  # User-friendly name (e.g., "Read File")
    description: str  # Short description


# Display name mappings for built-in tools
_TOOL_DISPLAY_NAMES = {
    "write_file": "Write File",
    "edit_file": "Edit File",
    "read_file": "Read File",
    "list_files": "List Files",
    "search": "Search",
    "run_command": "Run Command",
    "list_processes": "List Processes",
    "get_process_output": "Get Process Output",
    "kill_process": "Kill Process",
    "fetch_url": "Fetch URL",
    "web_search": "Web Search",
    "notebook_edit": "Edit Notebook",
    "ask_user": "Ask User",
    "write_todos": "Write Todos",
    "update_todo": "Update Todo",
    "complete_todo": "Complete Todo",
    "list_todos": "List Todos",
    "open_browser": "Open Browser",
    "capture_screenshot": "Capture Screenshot",
    "analyze_image": "Analyze Image",
    "capture_web_screenshot": "Capture Web Screenshot",
    "read_pdf": "Read PDF",
    "find_symbol": "Find Symbol",
    "find_referencing_symbols": "Find References",
    "insert_before_symbol": "Insert Before Symbol",
    "insert_after_symbol": "Insert After Symbol",
    "replace_symbol_body": "Replace Symbol Body",
    "rename_symbol": "Rename Symbol",
    "task_complete": "Task Complete",
    "search_tools": "Search Tools",
    "invoke_skill": "Invoke Skill",
    "get_subagent_output": "Get Subagent Output",
    "spawn_subagent": "Spawn Subagent",
}


def get_available_tools(
    tool_schemas: List[dict] | None = None,
    exclude_mcp_tools: bool = True,
) -> List[ToolInfo]:
    """Get list of all available tools with metadata.

    Args:
        tool_schemas: Optional list of tool schemas. If not provided, uses
                     _BUILTIN_TOOL_SCHEMAS from tool_schema_builder.
        exclude_mcp_tools: If True, exclude MCP-related tools from the list.

    Returns:
        List of ToolInfo objects with tool metadata.
    """
    if tool_schemas is None:
        from swecli.core.agents.components.schemas import _BUILTIN_TOOL_SCHEMAS
        tool_schemas = _BUILTIN_TOOL_SCHEMAS

    tools: List[ToolInfo] = []

    # Tools to exclude from selection (MCP tools that are for discovery only)
    excluded_tools = {
        "search_tools",  # For discovering MCP tools
    } if exclude_mcp_tools else set()

    for schema in tool_schemas:
        function_data = schema.get("function", {})
        name = function_data.get("name", "")
        description = function_data.get("description", "")

        if not name or name in excluded_tools:
            continue

        # Get display name, or generate from snake_case
        display_name = _TOOL_DISPLAY_NAMES.get(
            name,
            " ".join(word.capitalize() for word in name.split("_"))
        )

        # Truncate long descriptions
        if len(description) > 60:
            description = description[:57] + "..."

        tools.append(ToolInfo(
            name=name,
            display_name=display_name,
            description=description,
        ))

    # Sort alphabetically by display name
    tools.sort(key=lambda t: t.display_name)

    return tools


def format_tools_for_frontmatter(selected_tools: List[str], all_tools: List[ToolInfo]) -> str:
    """Format selected tools for YAML frontmatter.

    Args:
        selected_tools: List of selected tool names.
        all_tools: List of all available ToolInfo objects.

    Returns:
        YAML-formatted tools string (either "*" or a list).
    """
    # Get all built-in tool names
    all_tool_names = {t.name for t in all_tools}

    # Check if all built-in tools are selected
    selected_set = set(selected_tools)
    if selected_set >= all_tool_names:
        return '"*"'

    # Format as YAML list
    lines = ["tools:"]
    for tool in sorted(selected_tools):
        lines.append(f"  - {tool}")
    return "\n".join(lines)


__all__ = ["ToolInfo", "get_available_tools", "format_tools_for_frontmatter"]
