"""Utilities for presenting human-friendly tool call information."""

from __future__ import annotations

import os
from pathlib import Path, PurePath
from typing import Any, Mapping, Tuple

from rich.text import Text

from opendev.ui_textual.style_tokens import GREY, PRIMARY

# Canonical set of argument keys that hold file/directory paths.
# Shared between TUI ToolDisplayService and Web WebSocketToolBroadcaster.
PATH_ARG_KEYS: set[str] = {
    "path",
    "file_path",
    "working_dir",
    "directory",
    "dir",
    "target",
    "image_path",
}

_TOOL_DISPLAY_PARTS: dict[str, tuple[str, str]] = {
    "read_file": ("Read", "file"),
    "read_pdf": ("Read", "pdf"),
    "write_file": ("Write", "file"),
    "edit_file": ("Edit", "file"),
    "list_files": ("List", "files"),
    "search": ("Search", "project"),
    "run_command": ("Bash", "command"),
    "get_process_output": ("Get Process Output", "process"),
    "list_processes": ("List Processes", "processes"),
    "kill_process": ("Kill Process", "process"),
    "fetch_url": ("Fetch", "url"),
    "open_browser": ("Open", "browser"),
    "capture_screenshot": ("Capture_Screenshot", "screenshot"),
    "capture_web_screenshot": ("Capture_Web_Screenshot", "page"),
    "analyze_image": ("Analyze_Image", "image"),
    "write_todos": ("Create", "todos"),
    "update_todo": ("Update_Todos", "todo"),
    "complete_todo": ("Complete_Todos", "todo"),
    "list_todos": ("List_Todos", "todos"),
    "clear_todos": ("Clear_Todos", "todos"),
    "spawn_subagent": ("Spawn", "subagent"),
    "docker_start": ("Starting", "Docker container"),
    "docker_stop": ("Stopping", "Docker container"),
    "docker_copy": ("Copying", "file to Docker"),
    "docker_input_files": ("Checking", "input files"),
    "docker_setup": ("Setting up", "Docker environment"),
    # LSP/Symbol tools
    "find_symbol": ("Find_Symbol", "symbol"),
    "find_referencing_symbols": ("Find_References_Symbol", "symbol"),
    "insert_before_symbol": ("Insert_Before_Symbol", "symbol"),
    "insert_after_symbol": ("Insert_After_Symbol", "symbol"),
    "replace_symbol_body": ("Replace_Symbol", "symbol"),
    "rename_symbol": ("Rename_Symbol", "symbol"),
    # Plan tool
    "present_plan": ("Present Plan", "plan"),
    # Other tools
    "notebook_edit": ("Edit", "notebook"),
    "ask_user": ("Ask", "user"),
    "web_search": ("Search", "web"),
    "get_subagent_output": ("Get Output", "subagent"),
    "task_complete": ("Complete", "task"),
    "invoke_skill": ("Skill", "skill"),
}

_PATH_HINT_KEYS = {"file_path", "path", "directory", "dir", "image_path", "working_dir", "target"}

_PRIMARY_ARG_MAP: dict[str, tuple[str, ...]] = {
    "read_file": ("file_path",),
    "read_pdf": ("file_path",),
    "write_file": ("file_path", "path"),
    "edit_file": ("file_path", "path"),
    "list_files": ("path", "directory"),
    "search": ("pattern", "query"),
    "run_command": ("command",),
    "get_process_output": ("pid", "command"),
    "kill_process": ("pid",),
    "fetch_url": ("url",),
    "open_browser": ("url",),
    "capture_screenshot": ("target", "path"),
    "capture_web_screenshot": ("url",),
    "analyze_image": ("image_path", "file_path"),
}

_MAX_SUMMARY_LEN = 150
_NESTED_KEY_PRIORITY = (
    "command",
    "file_path",
    "path",
    "target",
    "url",
    "pid",
    "process_id",
    "query",
    "pattern",
    "directory",
    "name",
    "title",
    "description",
)


def _fallback_parts(tool_name: str) -> tuple[str, str]:
    cleaned = tool_name.replace("-", "_")
    tokens = [token for token in cleaned.split("_") if token]
    if not tokens:
        return ("Call", "tool")
    verb = tokens[0].capitalize()
    if len(tokens) == 1:
        return (verb, "item")
    label = " ".join(tokens[1:])
    return (verb, label)


def get_tool_display_parts(tool_name: str) -> Tuple[str, str]:
    if tool_name.startswith("mcp__"):
        parts = tool_name.split("__", 2)
        if len(parts) == 3:
            return ("MCP", f"{parts[1]}/{parts[2]}")
        if len(parts) == 2:
            return ("MCP", parts[1])
        return ("MCP", "tool")
    if tool_name in _TOOL_DISPLAY_PARTS:
        return _TOOL_DISPLAY_PARTS[tool_name]
    return _fallback_parts(tool_name)


def _shorten_path(value: str) -> str:
    try:
        path = PurePath(value)
    except Exception:
        return value
    parts = path.parts
    if len(parts) <= 2:
        return str(path)
    return f".../{'/'.join(parts[-2:])}"


def _is_path_string(value: str, key: str | None = None) -> bool:
    if key in _PATH_HINT_KEYS:
        return True
    normalized = value.strip()
    if not normalized:
        return False
    if normalized.startswith(("./", "../", "~/", "/")):
        return True
    if "\\" in normalized:
        return True
    if len(normalized) > 2 and normalized[1] == ":":
        return True
    return False


def _normalize_path_display(value: str) -> str:
    try:
        expanded = os.path.expanduser(value.strip())
        # Only normalize paths that are already absolute
        # Leave relative paths as-is (important for Docker subagents where
        # we intentionally sanitize /Users/.../file.py → file.py)
        if os.path.isabs(expanded):
            return os.path.abspath(expanded)
        return expanded
    except Exception:
        return value


def _format_summary_value(value: Any, key: str | None = None) -> str:
    if value is None:
        return ""
    if isinstance(value, (int, float)):
        return f"{key}={value}" if key else str(value)
    if isinstance(value, str):
        display = value.strip()
        if not display:
            return ""
        if key in {"pid", "process_id"} and display.isdigit():
            return f"{key}={display}"
        display = display.replace("\n", " ")
        if _is_path_string(display, key):
            return _normalize_path_display(display)
        if len(display) > _MAX_SUMMARY_LEN:
            display = display[: _MAX_SUMMARY_LEN - 3] + "..."
        return display
    display = str(value)
    if len(display) > _MAX_SUMMARY_LEN:
        display = display[: _MAX_SUMMARY_LEN - 3] + "..."
    return f"{key}={display}" if key else display


def _summarize_nested_value(value: Any, key: str | None, seen: set[int] | None = None) -> str:
    if seen is None:
        seen = set()
    identity = id(value)
    if identity in seen:
        return ""
    seen.add(identity)
    if isinstance(value, Mapping):
        for preferred_key in _NESTED_KEY_PRIORITY:
            if preferred_key in value:
                nested = _summarize_nested_value(value[preferred_key], preferred_key, seen)
                if nested:
                    return nested
        for nested_key, nested_value in value.items():
            nested = _summarize_nested_value(nested_value, nested_key, seen)
            if nested:
                return nested
        return ""
    if isinstance(value, (list, tuple, set)):
        for item in value:
            nested = _summarize_nested_value(item, key, seen)
            if nested:
                return nested
        return ""
    return _format_summary_value(value, key)


def summarize_tool_arguments(tool_name: str, tool_args: Mapping[str, Any]) -> str:
    if not isinstance(tool_args, Mapping) or not tool_args:
        return ""
    primary_keys = _PRIMARY_ARG_MAP.get(tool_name, ())
    for key in primary_keys:
        if key in tool_args:
            summary = _summarize_nested_value(tool_args[key], key)
            if summary:
                return summary
    for key, value in tool_args.items():
        if isinstance(value, str) and value.strip():
            return _format_summary_value(value, key)
    return ""


def build_tool_call_text(tool_name: str, tool_args: Mapping[str, Any]) -> Text:
    # Use the same enhanced formatting for rich text display
    formatted = format_tool_call(tool_name, tool_args)

    # Parse the formatted string to add styling
    if "(" in formatted and formatted.endswith(")"):
        tool_part, params_part = formatted.split("(", 1)
        params_part = params_part[:-1]  # Remove closing parenthesis

        # Strip trailing space from tool_part to avoid double space
        text = Text(tool_part.rstrip(), style=PRIMARY)
        if params_part:
            text.append(f" ({params_part})", style=GREY)
        return text
    else:
        return Text(formatted)


def format_tool_call(tool_name: str, tool_args: Mapping[str, Any]) -> str:
    # Enhanced formatting for search_tools (MCP tool discovery)
    if tool_name == "search_tools":
        if not tool_args:
            return "Search_Tools"

        query = tool_args.get("query", "")
        detail_level = tool_args.get("detail_level", "brief")
        server = tool_args.get("server", "")

        params = []
        if query:
            params.append(f'"{query}"')
        if detail_level and detail_level != "brief":
            params.append(f'detail: "{detail_level}"')
        if server:
            params.append(f'server: "{server}"')

        if params:
            return f"Search_Tools({', '.join(params)})"
        return "Search_Tools"

    # Enhanced formatting for Search tool
    elif tool_name == "search" and tool_args:
        params = []
        if "pattern" in tool_args and tool_args["pattern"]:
            params.append(f'pattern: "{tool_args["pattern"]}"')
        # Show search type (text or ast)
        search_type = tool_args.get("type", "text")
        params.append(f'type: "{search_type}"')
        if "lang" in tool_args and tool_args["lang"]:
            params.append(f'lang: "{tool_args["lang"]}"')
        if "glob" in tool_args and tool_args["glob"]:
            params.append(f'glob: "{tool_args["glob"]}"')
        if "output_mode" in tool_args and tool_args["output_mode"]:
            params.append(f'output_mode: "{tool_args["output_mode"]}"')
        if "path" in tool_args and tool_args["path"] and tool_args["path"] != ".":
            params.append(f'path: "{tool_args["path"]}"')

        if params:
            return f"Search({', '.join(params)})"

    # Enhanced formatting for Web Fetch tool
    elif tool_name == "fetch_url" and tool_args:
        params = []
        if "url" in tool_args and tool_args["url"]:
            params.append(f'url: "{tool_args["url"]}"')
        if "deep_crawl" in tool_args and tool_args["deep_crawl"]:
            params.append(f'deep_crawl: {tool_args["deep_crawl"]}')
            if "max_depth" in tool_args and tool_args["max_depth"] != 1:
                params.append(f'max_depth: {tool_args["max_depth"]}')
            if "max_pages" in tool_args and tool_args["max_pages"]:
                params.append(f'max_pages: {tool_args["max_pages"]}')
            if "crawl_strategy" in tool_args and tool_args["crawl_strategy"] != "best_first":
                params.append(f'crawl_strategy: "{tool_args["crawl_strategy"]}"')
        if "extract_text" in tool_args and not tool_args["extract_text"]:
            params.append(f'extract_text: {tool_args["extract_text"]}')
        if "include_external" in tool_args and tool_args["include_external"]:
            params.append(f'include_external: {tool_args["include_external"]}')

        if params:
            return f"Fetch({', '.join(params)})"

    # Enhanced formatting for spawn_subagent tool
    elif tool_name == "spawn_subagent" and tool_args:
        subagent_type = tool_args.get("subagent_type", "general-purpose")
        description = tool_args.get("description", "")
        # Show full description without truncation, no [Docker] suffix
        # Docker is shown as a separate step with spinner
        # Just show "Explore(description)" format, no "Spawn[]" wrapper
        if description:
            return f"{subagent_type}({description})"
        return subagent_type

    # Docker container startup
    elif tool_name == "docker_start" and tool_args:
        return "Starting Docker container"

    # Docker container stop
    elif tool_name == "docker_stop":
        container = tool_args.get("container", "") if tool_args else ""
        if container:
            return f"Stopping Docker container ({container})"
        return "Stopping Docker container"

    # Docker file copy
    elif tool_name == "docker_copy" and tool_args:
        filename = tool_args.get("file", "file")
        return f"Copying {filename} to Docker"

    # Docker input files check (always shown for transparency)
    elif tool_name == "docker_input_files" and tool_args:
        count = tool_args.get("count", 0)
        if count > 0:
            files = tool_args.get("files", [])
            file_list = ", ".join(files[:3])  # Limit to first 3 files
            if len(files) > 3:
                file_list += f" (+{len(files) - 3} more)"
            return f"Copying {count} file(s) to Docker: {file_list}"
        return "Checking input files"

    # Docker setup steps (git install, clone, etc.)
    elif tool_name == "docker_setup" and tool_args:
        step = tool_args.get("step", "Setting up...")
        return step

    # Enhanced formatting for update_todo tool - show todo-N format
    elif tool_name == "update_todo" and tool_args:
        verb, _ = get_tool_display_parts(tool_name)
        todo_id = tool_args.get("id", "?")
        # Normalize ID to "todo-N" format
        if str(todo_id).isdigit():
            # Convert 0-based index to 1-based todo-N format
            todo_id = f"todo-{int(todo_id) + 1}"
        elif not str(todo_id).startswith("todo-"):
            todo_id = f"todo-{todo_id}"
        return f"{verb} ({todo_id})"

    # Enhanced formatting for complete_todo tool - show todo-N format
    elif tool_name == "complete_todo" and tool_args:
        verb, _ = get_tool_display_parts(tool_name)
        todo_id = tool_args.get("id", "?")
        if str(todo_id).isdigit():
            todo_id = f"todo-{int(todo_id) + 1}"
        elif not str(todo_id).startswith("todo-"):
            todo_id = f"todo-{todo_id}"
        return f"{verb} ({todo_id})"

    # Enhanced formatting for bash/run commands - show working_dir for Docker
    elif tool_name == "run_command" and tool_args:
        verb, _ = get_tool_display_parts(tool_name)
        command = tool_args.get("command", "")
        working_dir = tool_args.get("working_dir", "")

        # Truncate long commands for display
        max_cmd_len = 60
        cmd_display = command.replace("\n", " ").strip()
        if len(cmd_display) > max_cmd_len:
            cmd_display = cmd_display[: max_cmd_len - 3] + "..."

        # If working_dir has Docker prefix (e.g., [uv:abc123]:/workspace), show it
        if working_dir and working_dir.startswith("["):
            return f"{verb} ({working_dir} {cmd_display})"
        return f"{verb} ({cmd_display})"

    # Enhanced formatting for list_files tool - show pattern and other params
    elif tool_name == "list_files" and tool_args:
        verb, _ = get_tool_display_parts(tool_name)
        path = tool_args.get("path", ".")
        pattern = tool_args.get("pattern", "")
        max_results = tool_args.get("max_results")

        if pattern:
            # Pattern mode: show path and pattern
            params = f'path: "{path}", pattern: "{pattern}"'
            if max_results and max_results != 100:
                params += f", max_results: {max_results}"
            return f"{verb}({params})"
        else:
            # Tree mode: just show path (default behavior)
            return f"{verb}({path})"

    # Plan tool
    elif tool_name == "present_plan":
        plan_path = tool_args.get("plan_file_path", "")
        if plan_path:
            return f"Present Plan({PurePath(plan_path).name})"
        return "Present Plan"

    # Default formatting for other tools
    verb, label = get_tool_display_parts(tool_name)
    summary = summarize_tool_arguments(tool_name, tool_args)
    if summary:
        return f"{verb}({summary})"
    if label:
        return f"{verb}({label})"
    return verb
