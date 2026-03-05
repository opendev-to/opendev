"""Rename symbol tool implementation.

This tool renames a symbol across the entire workspace using LSP refactoring.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from swecli.core.context_engineering.tools.lsp import SymbolRetriever


def handle_rename_symbol(arguments: dict[str, Any]) -> dict[str, Any]:
    """Handle the rename_symbol tool call.

    Args:
        arguments: Tool arguments containing:
            - symbol_name: Name of the symbol to rename
            - file_path: Path to file where symbol is defined
            - new_name: New name for the symbol

    Returns:
        Tool result with success status and modified files
    """
    symbol_name = arguments.get("symbol_name", "")
    file_path = arguments.get("file_path", "")
    new_name = arguments.get("new_name", "")

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

    if not new_name:
        return {
            "success": False,
            "error": "new_name is required",
            "output": None,
        }

    # Validate new_name is a valid identifier
    if not _is_valid_identifier(new_name):
        return {
            "success": False,
            "error": f"Invalid identifier: {new_name}",
            "output": None,
        }

    try:
        return _rename_symbol(symbol_name, file_path, new_name)
    except Exception as e:
        return {
            "success": False,
            "error": f"Failed to rename symbol: {e}",
            "output": None,
        }


def _rename_symbol(
    symbol_name: str,
    file_path: str,
    new_name: str,
) -> dict[str, Any]:
    """Implementation of rename_symbol."""
    path = Path(file_path).resolve()

    if not path.exists():
        return {
            "success": False,
            "error": f"File not found: {file_path}",
            "output": None,
        }

    workspace_root = path.parent
    retriever = SymbolRetriever(workspace_root=workspace_root)

    # Get workspace edit from LSP
    workspace_edit = retriever.rename_symbol_by_name(
        symbol_name, file_path, new_name
    )

    if workspace_edit is None:
        return {
            "success": False,
            "error": f"Rename failed - symbol '{symbol_name}' not found or rename not supported",
            "output": None,
        }

    if not workspace_edit:
        return {
            "success": False,
            "error": "Rename returned no changes",
            "output": None,
        }

    # Apply the edits
    modified_files = []
    total_changes = 0

    for edit_file, edits in workspace_edit.items():
        try:
            edit_path = Path(edit_file)
            if not edit_path.exists():
                continue

            content = edit_path.read_text()
            lines = content.splitlines(keepends=True)

            # Sort edits in reverse order (bottom to top) to preserve line numbers
            sorted_edits = sorted(
                edits,
                key=lambda e: (e["start_line"], e["start_character"]),
                reverse=True,
            )

            for edit in sorted_edits:
                lines = _apply_edit(
                    lines,
                    edit["start_line"],
                    edit["start_character"],
                    edit["end_line"],
                    edit["end_character"],
                    edit["new_text"],
                )
                total_changes += 1

            # Write back
            edit_path.write_text("".join(lines))
            modified_files.append(str(edit_file))

        except Exception as e:
            return {
                "success": False,
                "error": f"Failed to apply edit to {edit_file}: {e}",
                "output": None,
            }

    # Format output
    output_lines = [
        f"Successfully renamed '{symbol_name}' to '{new_name}'",
        f"Modified {len(modified_files)} file(s) with {total_changes} change(s):",
        "",
    ]

    for f in modified_files:
        # Try to make path relative
        try:
            rel_path = Path(f).relative_to(workspace_root)
        except ValueError:
            rel_path = Path(f)
        output_lines.append(f"  - {rel_path}")

    return {
        "success": True,
        "output": "\n".join(output_lines),
        "modified_files": modified_files,
        "total_changes": total_changes,
    }


def _apply_edit(
    lines: list[str],
    start_line: int,
    start_char: int,
    end_line: int,
    end_char: int,
    new_text: str,
) -> list[str]:
    """Apply a text edit to the lines."""
    # Handle empty file
    if not lines:
        return [new_text]

    # Ensure lines list is long enough
    while len(lines) <= max(start_line, end_line):
        lines.append("\n")

    result = []

    # Lines before the edit
    for i in range(start_line):
        result.append(lines[i])

    # The edit itself
    if start_line == end_line:
        # Single line edit
        line = lines[start_line]
        new_line = line[:start_char] + new_text + line[end_char:]
        result.append(new_line)
    else:
        # Multi-line edit
        first_line = lines[start_line]
        last_line = lines[end_line] if end_line < len(lines) else ""

        new_line = first_line[:start_char] + new_text + last_line[end_char:]
        result.append(new_line)

    # Lines after the edit
    for i in range(end_line + 1, len(lines)):
        result.append(lines[i])

    return result


def _is_valid_identifier(name: str) -> bool:
    """Check if a string is a valid identifier in most languages."""
    if not name:
        return False

    # Check first character (letter or underscore)
    if not (name[0].isalpha() or name[0] == "_"):
        return False

    # Check rest (alphanumeric or underscore)
    for char in name[1:]:
        if not (char.isalnum() or char == "_"):
            return False

    return True
