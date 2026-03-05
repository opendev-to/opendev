"""Insert before/after symbol tool implementation.

This tool inserts code before or after a specified symbol.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from swecli.core.context_engineering.tools.lsp import SymbolRetriever


def handle_insert_before_symbol(arguments: dict[str, Any]) -> dict[str, Any]:
    """Handle the insert_before_symbol tool call.

    Args:
        arguments: Tool arguments containing:
            - symbol_name: Name of the symbol to insert before
            - file_path: Path to file containing the symbol
            - content: Content to insert

    Returns:
        Tool result with success status
    """
    return _handle_insert_symbol(arguments, position="before")


def handle_insert_after_symbol(arguments: dict[str, Any]) -> dict[str, Any]:
    """Handle the insert_after_symbol tool call.

    Args:
        arguments: Tool arguments containing:
            - symbol_name: Name of the symbol to insert after
            - file_path: Path to file containing the symbol
            - content: Content to insert

    Returns:
        Tool result with success status
    """
    return _handle_insert_symbol(arguments, position="after")


def _handle_insert_symbol(
    arguments: dict[str, Any],
    position: str,
) -> dict[str, Any]:
    """Handle insert_before/after_symbol tool calls.

    Args:
        arguments: Tool arguments
        position: "before" or "after"

    Returns:
        Tool result
    """
    symbol_name = arguments.get("symbol_name", "")
    file_path = arguments.get("file_path", "")
    content = arguments.get("content", "")

    if not symbol_name:
        return {
            "success": False,
            "error": "symbol_name is required",
            "output": None,
        }

    if not file_path:
        return {
            "success": False,
            "error": "file_path is required",
            "output": None,
        }

    if not content:
        return {
            "success": False,
            "error": "content is required",
            "output": None,
        }

    try:
        return _insert_symbol(symbol_name, file_path, content, position)
    except Exception as e:
        return {
            "success": False,
            "error": f"Failed to insert content: {e}",
            "output": None,
        }


def _insert_symbol(
    symbol_name: str,
    file_path: str,
    content: str,
    position: str,
) -> dict[str, Any]:
    """Implementation of insert_symbol."""
    path = Path(file_path).resolve()

    if not path.exists():
        return {
            "success": False,
            "error": f"File not found: {file_path}",
            "output": None,
        }

    workspace_root = path.parent
    retriever = SymbolRetriever(workspace_root=workspace_root)

    # Find the symbol
    symbols = retriever.find_symbol(symbol_name, file_path)

    if not symbols:
        return {
            "success": False,
            "error": f"Symbol not found: {symbol_name}",
            "output": None,
        }

    symbol = symbols[0]

    # Read file content
    try:
        file_content = path.read_text()
    except Exception as e:
        return {
            "success": False,
            "error": f"Failed to read file: {e}",
            "output": None,
        }

    lines = file_content.splitlines(keepends=True)

    # Determine insertion point
    if position == "before":
        insert_line = symbol.start_line
        # Find the indentation of the symbol
        if insert_line < len(lines):
            indent = _get_indentation(lines[insert_line])
        else:
            indent = ""
    else:  # after
        insert_line = symbol.end_line + 1
        # Use same indentation as symbol start
        if symbol.start_line < len(lines):
            indent = _get_indentation(lines[symbol.start_line])
        else:
            indent = ""

    # Prepare content with proper indentation
    content_lines = content.splitlines(keepends=True)
    # Ensure content ends with newline
    if content_lines and not content_lines[-1].endswith("\n"):
        content_lines[-1] += "\n"

    # Add blank line for separation if needed
    if position == "after":
        content_to_insert = "\n" + "".join(content_lines)
    else:
        content_to_insert = "".join(content_lines) + "\n"

    # Insert content
    if insert_line >= len(lines):
        # Append to end
        new_content = file_content.rstrip("\n") + "\n" + content_to_insert
    else:
        # Insert at line
        lines.insert(insert_line, content_to_insert)
        new_content = "".join(lines)

    # Write back
    try:
        path.write_text(new_content)
    except Exception as e:
        return {
            "success": False,
            "error": f"Failed to write file: {e}",
            "output": None,
        }

    # Build output
    action = "before" if position == "before" else "after"
    output = (
        f"Successfully inserted content {action} symbol '{symbol_name}' "
        f"at line {insert_line + 1} in {file_path}"
    )

    return {
        "success": True,
        "output": output,
        "file_path": str(path),
        "insert_line": insert_line + 1,
        "symbol": symbol.to_dict(),
    }


def _get_indentation(line: str) -> str:
    """Extract leading whitespace from a line."""
    stripped = line.lstrip()
    return line[: len(line) - len(stripped)]
