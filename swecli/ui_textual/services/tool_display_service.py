"""Unified tool display service - single source of truth for tool formatting."""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any, Dict, Optional

from rich.text import Text

from swecli.ui_textual.services.display_data import BashOutputData, ToolResultData
from swecli.ui_textual.utils.tool_display import build_tool_call_text


# Path argument keys that should be resolved to absolute paths
_PATH_ARG_KEYS = {"path", "file_path", "working_dir", "directory", "dir", "target"}


class ToolDisplayService:
    """Single source of truth for tool display formatting.

    This service provides unified formatting logic for tool calls and results,
    ensuring consistent display behavior between live execution and session replay.

    Attributes:
        BASH_HEAD_LINES: Number of lines to show at the start of bash output.
        BASH_TAIL_LINES: Number of lines to show at the end of bash output.
        NESTED_HEAD_LINES: Number of lines to show at the start of nested output.
        NESTED_TAIL_LINES: Number of lines to show at the end of nested output.
        GENERIC_MAX_LINES: Maximum lines for generic tool output.
        GENERIC_MAX_CHARS: Maximum characters for generic tool output.
    """

    # Unified truncation constants
    BASH_HEAD_LINES = 5
    BASH_TAIL_LINES = 5
    NESTED_HEAD_LINES = 3
    NESTED_TAIL_LINES = 3
    GENERIC_MAX_LINES = 6
    GENERIC_MAX_CHARS = 400

    def __init__(self, working_dir: Optional[Path] = None):
        """Initialize the ToolDisplayService.

        Args:
            working_dir: Working directory for resolving relative paths.
                         If None, uses current working directory.
        """
        self._working_dir = working_dir or Path.cwd()
        # Lazy import to avoid circular dependencies
        self._formatter: Any = None

    @property
    def formatter(self):
        """Lazy-load the StyleFormatter to avoid circular imports."""
        if self._formatter is None:
            from swecli.ui_textual.formatters.style_formatter import StyleFormatter

            self._formatter = StyleFormatter()
        return self._formatter

    def format_tool_header(self, tool_name: str, tool_args: Dict[str, Any]) -> Text:
        """Format tool call header with resolved paths.

        Args:
            tool_name: Name of the tool being called.
            tool_args: Arguments for the tool call.

        Returns:
            Rich Text object with formatted tool call header.
        """
        resolved = self.resolve_paths(tool_args)
        return build_tool_call_text(tool_name, resolved)

    def format_tool_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> ToolResultData:
        """Format tool result with consistent truncation.

        Args:
            tool_name: Name of the tool that was executed.
            tool_args: Arguments that were used.
            result: Result of the tool execution.

        Returns:
            ToolResultData with formatted result ready for display.
        """
        # Handle string results by converting to dict format
        if isinstance(result, str):
            result = {"success": True, "output": result}

        # Check for special states
        # Support both dict and dataclass objects (e.g., HttpResult)
        success = (
            result.get("success", True) if isinstance(result, dict)
            else getattr(result, "success", True)
        )
        is_interrupted = (
            result.get("interrupted") if isinstance(result, dict)
            else getattr(result, "interrupted", False)
        )
        is_rejected = (
            result.get("_approved") is False if isinstance(result, dict)
            else getattr(result, "_approved", True) is False
        )

        if is_interrupted:
            # Don't return any lines for interrupted operations
            # The interrupt message is already shown by ui_callback.on_interrupt()
            # Returning lines here would cause them to be displayed as tool results
            return ToolResultData(
                success=False,
                lines=[],  # Empty lines - no redundant message needed
                is_interrupted=True,
            )

        if is_rejected:
            return ToolResultData(
                success=False,
                lines=["Operation rejected by user"],
                is_rejected=True,
            )

        # Detect special types and format accordingly
        if tool_name in ("bash_execute", "run_command", "Bash"):
            return self._format_bash_result(tool_name, tool_args, result)
        elif tool_name == "present_plan":
            return self._format_plan_mode_result(tool_name, result)
        elif tool_name == "ask_user":
            return self._format_ask_user_result(result)
        elif tool_name == "spawn_subagent":
            return self._format_subagent_result(result)
        else:
            return self._format_generic_result(tool_name, tool_args, result)

    def resolve_paths(self, args: Dict[str, Any]) -> Dict[str, Any]:
        """Resolve relative paths to absolute paths for display.

        This is the UNIFIED path resolution logic used by both live and replay modes.

        Args:
            args: Tool arguments dictionary.

        Returns:
            Copy of args with paths resolved to absolute paths.
        """
        result = dict(args)
        for key in _PATH_ARG_KEYS:
            if key in result and isinstance(result[key], str):
                path = result[key]
                # Skip if already absolute or has special prefix (like docker://[...]:)
                if path.startswith("/") or path.startswith("["):
                    continue
                # Resolve relative path
                if path == "." or path == "":
                    result[key] = str(self._working_dir)
                else:
                    clean_path = path.lstrip("./")
                    result[key] = str(self._working_dir / clean_path)
        return result

    def truncate_output(
        self,
        text: str,
        mode: str = "generic",
        head_lines: Optional[int] = None,
        tail_lines: Optional[int] = None,
    ) -> tuple[str, bool, int]:
        """Apply consistent truncation rules.

        Args:
            text: The text to truncate.
            mode: Truncation mode - "bash", "nested", or "generic".
            head_lines: Override head lines count (optional).
            tail_lines: Override tail lines count (optional).

        Returns:
            Tuple of (truncated_text, is_truncated, hidden_count).
        """
        if not text:
            return "", False, 0

        lines = text.splitlines()

        # Determine limits based on mode
        if mode == "bash":
            head = head_lines if head_lines is not None else self.BASH_HEAD_LINES
            tail = tail_lines if tail_lines is not None else self.BASH_TAIL_LINES
        elif mode == "nested":
            head = head_lines if head_lines is not None else self.NESTED_HEAD_LINES
            tail = tail_lines if tail_lines is not None else self.NESTED_TAIL_LINES
        else:  # generic
            # For generic, use max_lines without head/tail split
            max_lines = head_lines if head_lines is not None else self.GENERIC_MAX_LINES
            max_chars = tail_lines if tail_lines is not None else self.GENERIC_MAX_CHARS

            if len(lines) > max_lines:
                truncated = "\n".join(lines[:max_lines])
                return truncated + "\n... (truncated)", True, len(lines) - max_lines

            result = "\n".join(lines)
            if len(result) > max_chars:
                return result[:max_chars].rstrip() + "\n... (truncated)", True, 0

            return result, False, 0

        # Head/tail truncation for bash and nested modes
        total = head + tail
        if len(lines) <= total:
            return "\n".join(lines), False, 0

        hidden = len(lines) - total
        head_part = lines[:head]
        tail_part = lines[-tail:] if tail > 0 else []
        middle = [f"... ({hidden} lines hidden) ..."]

        result = "\n".join(head_part + middle + tail_part)
        return result, True, hidden

    def normalize_arguments(self, tool_args: Any) -> Dict[str, Any]:
        """Ensure tool arguments are represented as a dictionary.

        Args:
            tool_args: Raw tool arguments (can be dict, str, or other).

        Returns:
            Normalized dictionary of arguments.
        """
        if isinstance(tool_args, dict):
            result = tool_args
        elif isinstance(tool_args, str):
            try:
                parsed = json.loads(tool_args)
                if isinstance(parsed, dict):
                    result = parsed
                else:
                    result = {"value": parsed}
            except json.JSONDecodeError:
                result = {"value": tool_args}
        else:
            result = {"value": tool_args}

        # Normalize URLs for display (fix common malformations)
        if "url" in result and isinstance(result["url"], str):
            url = result["url"].strip()
            # Fix: https:/domain.com -> https://domain.com
            if url.startswith("https:/") and not url.startswith("https://"):
                result["url"] = url.replace("https:/", "https://", 1)
            elif url.startswith("http:/") and not url.startswith("http://"):
                result["url"] = url.replace("http:/", "http://", 1)
            # Add protocol if missing
            elif not url.startswith(("http://", "https://")):
                result["url"] = f"https://{url}"

        return result

    def _format_bash_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> ToolResultData:
        """Format bash command result.

        Args:
            tool_name: Name of the bash tool.
            tool_args: Command arguments.
            result: Execution result.

        Returns:
            ToolResultData with bash-specific formatting.
        """
        is_error = not result.get("success", True)
        command = tool_args.get("command", "")
        working_dir = tool_args.get("working_dir", str(self._working_dir))

        # Handle background tasks
        background_task_id = result.get("background_task_id")
        if background_task_id:
            return ToolResultData(
                success=True,
                lines=[f"Running in background ({background_task_id})"],
                special_type="bash",
            )

        # Get output - use "output" key (combined stdout+stderr) or fallback
        output = result.get("output") or result.get("stdout") or ""
        stderr = result.get("stderr") or ""

        # Combine stdout and stderr for display
        if stderr and stderr not in output:
            output = (output + "\n" + stderr).strip() if output else stderr

        # Filter out placeholder messages
        if output in ("Command executed", "Command execution failed"):
            output = ""

        # Add OK prefix for successful commands (Claude Code style)
        if not is_error:
            cmd_name = command.split()[0] if command else "command"
            ok_line = f"OK: {cmd_name} ran successfully"
            if output:
                output = ok_line + "\n" + output
            else:
                output = ok_line

        # Add fallback for failed commands with empty output
        if is_error and not output:
            output = "Command failed"

        # Apply truncation
        truncated_output, is_truncated, hidden_count = self.truncate_output(output, mode="bash")

        return ToolResultData(
            success=not is_error,
            lines=[],  # Bash uses special bash_data, not lines
            special_type="bash",
            bash_data=BashOutputData(
                output=truncated_output,
                is_error=is_error,
                command=command,
                working_dir=working_dir,
                is_truncated=is_truncated,
                hidden_count=hidden_count,
            ),
        )

    def _format_ask_user_result(self, result: Dict[str, Any]) -> ToolResultData:
        """Format ask_user result.

        Args:
            result: The ask_user result containing questions and answers.

        Returns:
            ToolResultData with ask_user formatting.
        """
        questions_context = result.get("questions_context", [])
        answers = result.get("answers", {})
        cancelled = result.get("cancelled", False)

        lines = []
        if questions_context:
            for idx, q in enumerate(questions_context):
                header = q.get("header") or f"Q{idx + 1}"
                question = q.get("question", "")
                answer = answers.get(str(idx), "(not answered)")
                if cancelled:
                    answer = "(cancelled)"
                lines.append(f"[{header}] {question} -> {answer}")
        else:
            # Fallback to output text
            output = result.get("output", "")
            if output:
                lines.append(output)

        return ToolResultData(
            success=True,
            lines=lines,
            special_type="ask_user",
        )

    def _format_subagent_result(self, result: Dict[str, Any]) -> ToolResultData:
        """Format spawn_subagent result.

        Args:
            result: The subagent result.

        Returns:
            ToolResultData with subagent formatting.
        """
        nested_calls = result.get("nested_tool_calls", [])
        content = result.get("content", "")

        lines = []
        if content:
            lines.append(content)

        return ToolResultData(
            success=result.get("success", True),
            lines=lines,
            special_type="subagent",
            nested_calls=nested_calls,
        )

    def _format_plan_mode_result(
        self,
        tool_name: str,
        result: Dict[str, Any],
    ) -> ToolResultData:
        """Format present_plan tool results.

        Args:
            tool_name: "present_plan".
            result: Execution result dict.

        Returns:
            ToolResultData with a concise summary line.
        """
        if result.get("plan_approved"):
            line = "Plan approved — proceeding with implementation"
        elif result.get("requires_modification"):
            line = "Plan needs revision"
        elif result.get("plan_rejected"):
            line = "Plan rejected"
        elif not result.get("success", False):
            line = result.get("error", "Plan approval failed")
        else:
            line = result.get("output", "Plan completed")

        return ToolResultData(
            success=result.get("success", False),
            lines=[line],
        )

    def _format_generic_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> ToolResultData:
        """Format generic tool result using StyleFormatter.

        Args:
            tool_name: Name of the tool.
            tool_args: Tool arguments.
            result: Execution result.

        Returns:
            ToolResultData with formatted lines.
        """
        success = result.get("success", True)

        # Use StyleFormatter to get formatted output
        formatted = self.formatter.format_tool_result(tool_name, tool_args, result)

        # Extract result lines from formatted output
        lines = []
        continuation_lines = []
        diff_text = ""

        # Check for diff in edit_file results
        if tool_name == "edit_file" and result.get("success"):
            diff_text = result.get("diff", "")

        if isinstance(formatted, str):
            raw_lines = formatted.splitlines()
            first_result_line_seen = False

            for line in raw_lines:
                stripped = line.strip()
                if stripped.startswith("⎿"):
                    result_text = stripped.lstrip("⎿").strip()
                    if result_text:
                        if not first_result_line_seen:
                            # First result line goes to main lines
                            first_result_line_seen = True
                            lines.append(result_text)
                        else:
                            # Subsequent lines go to continuation
                            # Skip @@ header lines
                            if not result_text.startswith("@@"):
                                continuation_lines.append(result_text)

        return ToolResultData(
            success=success,
            lines=lines,
            continuation_lines=continuation_lines,
            diff_text=diff_text,
        )

    def extract_result_lines(self, formatted: str) -> list[str]:
        """Extract result lines from StyleFormatter output.

        This is a utility method for extracting display lines from
        the StyleFormatter's formatted output.

        Args:
            formatted: The formatted output string from StyleFormatter.

        Returns:
            List of result lines (without the bullet prefix).
        """
        lines = []
        in_result = False

        if not isinstance(formatted, str):
            return lines

        for line in formatted.splitlines():
            stripped = line.strip()
            if stripped.startswith("⎿"):
                in_result = True
                result_text = stripped.lstrip("⎿").strip()
                if result_text:
                    lines.append(result_text)
            elif in_result and stripped:
                # Continuation line after first result
                lines.append(stripped)

        return lines
