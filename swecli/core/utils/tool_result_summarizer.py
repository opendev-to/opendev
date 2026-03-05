"""Intelligent tool result summarization for LLM context."""

from typing import Any


def summarize_tool_result(tool_name: str, result: Any, error: str | None = None) -> str:
    """Create a concise 1-2 line summary of a tool result for LLM context.

    This prevents context bloat while maintaining semantic meaning for the LLM.

    Args:
        tool_name: Name of the tool that was executed
        result: The full tool result
        error: Error message if tool failed

    Returns:
        Concise summary suitable for LLM context (typically 50-200 chars)
    """
    if error:
        # For errors, keep them concise
        error_msg = str(error)[:200]
        return f"❌ Error: {error_msg}"

    if not result:
        return "✓ Success (no output)"

    result_str = str(result)

    # File operations
    if tool_name in ("read_file", "Read"):
        lines = result_str.count("\n") + 1
        chars = len(result_str)
        return f"✓ Read file ({lines} lines, {chars} chars)"

    if tool_name in ("write_file", "Write"):
        return f"✓ File written successfully"

    if tool_name in ("edit_file", "Edit"):
        return f"✓ File edited successfully"

    if tool_name in ("delete_file", "Delete"):
        return f"✓ File deleted"

    # Search operations
    if tool_name in ("search", "Grep"):
        if "No matches found" in result_str or not result_str.strip():
            return "✓ Search completed (0 matches)"
        # Try to count matches
        match_count = result_str.count("\n") if result_str else 0
        return f"✓ Search completed ({match_count} matches found)"

    # Directory operations
    if tool_name in ("list_files", "list_directory", "List"):
        file_count = result_str.count("\n") + 1 if result_str else 0
        return f"✓ Listed directory ({file_count} items)"

    # Bash/command execution
    if tool_name in ("run_command", "Run", "bash_execute", "Bash"):
        lines = result_str.count("\n") + 1 if result_str else 0
        if lines > 10:
            return f"✓ Command executed ({lines} lines of output)"
        elif result_str and len(result_str) < 100:
            return f"✓ Output: {result_str[:100]}"
        else:
            return "✓ Command executed successfully"

    # Web operations
    if tool_name in ("fetch_url", "Fetch", "capture_web_screenshot"):
        return f"✓ Content fetched successfully"

    # Image operations
    if tool_name in ("analyze_image", "Analyze", "capture_screenshot"):
        return f"✓ Image processed successfully"

    # Thinking tool - return directive, not content (content would contaminate LLM response)
    # The actual thinking is displayed in UI via _thinking_content key
    if tool_name == "think":
        return "[Thinking captured. Now respond directly to the user.]"

    # Generic fallback
    if len(result_str) < 100:
        return f"✓ {result_str}"
    else:
        chars = len(result_str)
        lines = result_str.count("\n") + 1
        return f"✓ Success ({lines} lines, {chars} chars)"
