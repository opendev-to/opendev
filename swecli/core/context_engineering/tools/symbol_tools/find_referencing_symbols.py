"""Find referencing symbols tool implementation.

This tool finds all code locations that reference a given symbol.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from swecli.core.context_engineering.tools.lsp import SymbolRetriever


def handle_find_referencing_symbols(arguments: dict[str, Any]) -> dict[str, Any]:
    """Handle the find_referencing_symbols tool call.

    Args:
        arguments: Tool arguments containing:
            - symbol_name: Name of the symbol to find references for
            - file_path: Path to file where symbol is defined
            - include_declaration: Whether to include the declaration (default: True)

    Returns:
        Tool result with success status and reference locations
    """
    symbol_name = arguments.get("symbol_name", "")
    file_path = arguments.get("file_path", "")
    include_declaration = arguments.get("include_declaration", True)

    if not symbol_name:
        return {
            "success": False,
            "error": "symbol_name is required",
            "output": None,
        }

    if not file_path:
        return {
            "success": False,
            "error": "file_path is required to locate the symbol",
            "output": None,
        }

    try:
        return _find_references(symbol_name, file_path, include_declaration)
    except Exception as e:
        return {
            "success": False,
            "error": f"Failed to find references: {e}",
            "output": None,
        }


def _find_references(
    symbol_name: str,
    file_path: str,
    include_declaration: bool,
) -> dict[str, Any]:
    """Implementation of find_referencing_symbols."""
    path = Path(file_path).resolve()
    workspace_root = path.parent

    retriever = SymbolRetriever(workspace_root=workspace_root)

    references = retriever.find_references_by_name(
        symbol_name, file_path, include_declaration
    )

    if not references:
        return {
            "success": True,
            "output": f"No references found for '{symbol_name}'",
            "references": [],
        }

    # Group references by file
    by_file: dict[str, list[dict[str, Any]]] = {}
    for ref in references:
        ref_file = ref["file"]
        if ref_file not in by_file:
            by_file[ref_file] = []
        by_file[ref_file].append(ref)

    # Format output
    output_lines = [
        f"Found {len(references)} reference(s) to '{symbol_name}' "
        f"across {len(by_file)} file(s):\n"
    ]

    for ref_file, file_refs in by_file.items():
        # Make path relative if possible
        try:
            rel_path = Path(ref_file).relative_to(workspace_root)
        except ValueError:
            rel_path = Path(ref_file)

        output_lines.append(f"  {rel_path}:")

        for ref in file_refs:
            output_lines.append(f"    Line {ref['line']}, col {ref['character']}")

            # Try to show the line content
            try:
                file_content = Path(ref_file).read_text()
                lines = file_content.splitlines()
                line_idx = ref["line"] - 1  # Convert to 0-indexed
                if 0 <= line_idx < len(lines):
                    line = lines[line_idx].strip()
                    # Truncate if too long
                    if len(line) > 80:
                        line = line[:80] + "..."
                    output_lines.append(f"      {line}")
            except Exception:
                pass

        output_lines.append("")

    return {
        "success": True,
        "output": "\n".join(output_lines),
        "references": references,
        "file_count": len(by_file),
        "total_count": len(references),
    }
