"""Replace symbol body tool implementation.

This tool replaces the body of a symbol (function, class, etc.) with new content.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from swecli.core.context_engineering.tools.lsp import SymbolRetriever, Symbol, SymbolKind


def handle_replace_symbol_body(arguments: dict[str, Any]) -> dict[str, Any]:
    """Handle the replace_symbol_body tool call.

    Args:
        arguments: Tool arguments containing:
            - symbol_name: Name of the symbol to replace
            - file_path: Path to file containing the symbol
            - new_body: New body content for the symbol
            - preserve_signature: Whether to keep function/method signature (default: True)

    Returns:
        Tool result with success status
    """
    symbol_name = arguments.get("symbol_name", "")
    file_path = arguments.get("file_path", "")
    new_body = arguments.get("new_body", "")
    preserve_signature = arguments.get("preserve_signature", True)

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

    if not new_body:
        return {
            "success": False,
            "error": "new_body is required",
            "output": None,
        }

    try:
        return _replace_symbol_body(
            symbol_name, file_path, new_body, preserve_signature
        )
    except Exception as e:
        return {
            "success": False,
            "error": f"Failed to replace symbol body: {e}",
            "output": None,
        }


def _replace_symbol_body(
    symbol_name: str,
    file_path: str,
    new_body: str,
    preserve_signature: bool,
) -> dict[str, Any]:
    """Implementation of replace_symbol_body."""
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

    # Determine replacement range
    if preserve_signature and symbol.kind in {
        SymbolKind.FUNCTION,
        SymbolKind.METHOD,
        SymbolKind.CLASS,
    }:
        # Find end of signature (the colon for Python, opening brace for others)
        body_start_line, body_start_char = _find_body_start(
            lines, symbol.start_line, symbol.start_character, path.suffix
        )

        if body_start_line is None:
            # Fallback to replacing entire symbol
            replace_start_line = symbol.start_line
            replace_start_char = symbol.start_character
        else:
            replace_start_line = body_start_line
            replace_start_char = body_start_char
    else:
        # Replace entire symbol
        replace_start_line = symbol.start_line
        replace_start_char = symbol.start_character

    replace_end_line = symbol.end_line
    replace_end_char = symbol.end_character

    # Build new file content
    new_content = _replace_range(
        lines,
        replace_start_line,
        replace_start_char,
        replace_end_line,
        replace_end_char,
        new_body,
        preserve_signature,
    )

    # Write back
    try:
        path.write_text(new_content)
    except Exception as e:
        return {
            "success": False,
            "error": f"Failed to write file: {e}",
            "output": None,
        }

    output = (
        f"Successfully replaced body of '{symbol_name}' "
        f"(lines {symbol.start_line + 1}-{symbol.end_line + 1}) in {file_path}"
    )

    return {
        "success": True,
        "output": output,
        "file_path": str(path),
        "symbol": symbol.to_dict(),
    }


def _find_body_start(
    lines: list[str],
    start_line: int,
    start_char: int,
    file_extension: str,
) -> tuple[int | None, int | None]:
    """Find where the body of a function/class starts.

    For Python: after the colon and any docstring
    For other languages: after the opening brace

    Returns:
        (line, character) of body start, or (None, None) if not found
    """
    if file_extension in {".py", ".pyi", ".pyw"}:
        # Python - find colon then skip docstring
        for i in range(start_line, min(start_line + 10, len(lines))):
            line = lines[i]

            # Find colon
            colon_pos = line.find(":")
            if colon_pos >= 0:
                # Check if there's code after the colon on same line
                after_colon = line[colon_pos + 1 :].strip()
                if after_colon and not after_colon.startswith("#"):
                    # Single-line body
                    return i, colon_pos + 1

                # Multi-line body - return start of next line
                next_line = i + 1
                if next_line < len(lines):
                    # Skip empty lines and docstrings
                    while next_line < len(lines):
                        next_content = lines[next_line].strip()
                        if not next_content:
                            next_line += 1
                            continue
                        # Check for docstring
                        if next_content.startswith('"""') or next_content.startswith("'''"):
                            # Skip until docstring ends
                            quote = next_content[:3]
                            if next_content.count(quote) >= 2:
                                # Single-line docstring
                                next_line += 1
                                continue
                            # Multi-line docstring
                            next_line += 1
                            while next_line < len(lines):
                                if quote in lines[next_line]:
                                    next_line += 1
                                    break
                                next_line += 1
                            continue
                        break

                    return next_line, 0

                return i, colon_pos + 1

    else:
        # C-like languages - find opening brace
        for i in range(start_line, min(start_line + 20, len(lines))):
            line = lines[i]
            brace_pos = line.find("{")
            if brace_pos >= 0:
                return i, brace_pos + 1

    return None, None


def _replace_range(
    lines: list[str],
    start_line: int,
    start_char: int,
    end_line: int,
    end_char: int,
    new_body: str,
    preserve_signature: bool,
) -> str:
    """Replace a range in the file with new content."""
    result_parts = []

    # Content before the range
    for i in range(start_line):
        result_parts.append(lines[i])

    # Partial first line (if preserving signature)
    if preserve_signature and start_line < len(lines):
        result_parts.append(lines[start_line][:start_char])

    # Add new body
    # Ensure proper newline handling
    if new_body and not new_body.startswith("\n") and preserve_signature:
        result_parts.append("\n")

    result_parts.append(new_body)

    if new_body and not new_body.endswith("\n"):
        result_parts.append("\n")

    # Content after the range
    if end_line < len(lines):
        # Partial last line
        remaining = lines[end_line][end_char:]
        if remaining.strip():
            result_parts.append(remaining)

        # Lines after
        for i in range(end_line + 1, len(lines)):
            result_parts.append(lines[i])

    return "".join(result_parts)
