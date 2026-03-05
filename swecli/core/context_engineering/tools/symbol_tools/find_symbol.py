"""Find symbol tool implementation.

This tool finds symbols by name pattern using LSP.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from swecli.core.context_engineering.tools.lsp import SymbolRetriever


def handle_find_symbol(arguments: dict[str, Any]) -> dict[str, Any]:
    """Handle the find_symbol tool call.

    Args:
        arguments: Tool arguments containing:
            - symbol_name: Name path pattern (e.g., "MyClass.method", "my_func", "My*")
            - file_path: Optional file to search in. If None, searches workspace.

    Returns:
        Tool result with success status and found symbols
    """
    symbol_name = arguments.get("symbol_name", "")
    file_path = arguments.get("file_path")

    if not symbol_name:
        return {
            "success": False,
            "error": "symbol_name is required",
            "output": None,
        }

    try:
        return _find_symbol(symbol_name, file_path)
    except Exception as e:
        return {
            "success": False,
            "error": f"Failed to find symbol: {e}",
            "output": None,
        }


def _find_symbol(
    symbol_name: str,
    file_path: str | None,
) -> dict[str, Any]:
    """Implementation of find_symbol."""
    # Get workspace root from file path or current directory
    workspace_root = None
    if file_path:
        workspace_root = Path(file_path).parent
    else:
        workspace_root = Path.cwd()

    retriever = SymbolRetriever(workspace_root=workspace_root)

    symbols = retriever.find_symbol(symbol_name, file_path)

    if not symbols:
        return {
            "success": True,
            "output": f"No symbols found matching '{symbol_name}'",
            "symbols": [],
        }

    # Format output
    output_lines = [f"Found {len(symbols)} symbol(s) matching '{symbol_name}':\n"]

    for symbol in symbols:
        output_lines.append(
            f"  {symbol.kind_name} {symbol.name_path}"
        )
        output_lines.append(
            f"    File: {symbol.file_path}:{symbol.line_number}"
        )

        # Show a snippet of the symbol body
        try:
            body = symbol.get_body()
            if body:
                # Truncate long bodies
                preview = body[:200].strip()
                if len(body) > 200:
                    preview += "..."
                # Indent preview
                preview_lines = preview.split("\n")
                output_lines.append("    Preview:")
                for line in preview_lines[:5]:  # Max 5 lines
                    output_lines.append(f"      {line}")
                if len(preview_lines) > 5:
                    output_lines.append("      ...")
        except Exception:
            pass

        output_lines.append("")

    return {
        "success": True,
        "output": "\n".join(output_lines),
        "symbols": [s.to_dict() for s in symbols],
    }
