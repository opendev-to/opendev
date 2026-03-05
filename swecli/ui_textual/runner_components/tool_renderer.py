"""Tool rendering component for TextualRunner.

This module handles the rendering of tool calls, results, and nested tool calls
within the Textual conversation log. Uses ToolDisplayService for unified formatting.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from swecli.ui_textual.constants import TOOL_ERROR_SENTINEL
from swecli.ui_textual.services import ToolDisplayService
from swecli.ui_textual.utils.text_utils import summarize_error


class ToolRenderer:
    """Manages rendering of tool calls and results in the conversation log.

    This class handles the display formatting for standard tool calls, nested
    tool calls (subagents), and special handling for bash execution outputs.
    Uses ToolDisplayService for unified formatting between live and replay modes.
    """

    def __init__(self, working_dir: Path) -> None:
        """Initialize the ToolRenderer.

        Args:
            working_dir: Working directory used for path resolution in display.
        """
        self._working_dir = working_dir
        # Unified display service for consistent formatting
        self._display_service = ToolDisplayService(working_dir)

    def render_stored_tool_calls(self, conversation: Any, tool_calls: list[Any]) -> None:
        """Replay historical tool calls using the same logic as live display.

        Args:
            conversation: The conversation widget to render into.
            tool_calls: List of tool call objects to render.
        """
        if not tool_calls:
            return

        from swecli.ui_textual.formatters.style_formatter import StyleFormatter

        formatter = StyleFormatter()

        for tool_call in tool_calls:
            tool_name = getattr(tool_call, "name", "tool")
            try:
                parameters = self._coerce_tool_parameters(getattr(tool_call, "parameters", {}))
            except Exception:
                parameters = {}
            result = getattr(tool_call, "result", None)
            approved = getattr(tool_call, "approved", None)

            # Use unified service for path resolution and header formatting
            display = self._display_service.format_tool_header(tool_name, parameters)

            # Gap 8: Check approval status and show rejection
            if approved is False:
                conversation.add_tool_call(f"{display} [REJECTED]")
                if hasattr(conversation, "stop_tool_execution"):
                    conversation.stop_tool_execution(success=False)
                conversation.add_tool_result("⚠ Operation rejected by user")
                continue

            conversation.add_tool_call(display)
            if hasattr(conversation, "stop_tool_execution"):
                conversation.stop_tool_execution()

            # Handle result
            if isinstance(result, str):
                result = {"success": True, "output": result}

            # Gap 6: Display interrupted operations with indicator instead of skipping
            # Note: Don't add "Interrupted by user" message here - it's already shown
            # by ui_callback.on_interrupt() which displays the interrupt message
            # Check for interrupted flag in both dict and dataclass objects (e.g., HttpResult)
            interrupted = (
                result.get("interrupted") if isinstance(result, dict)
                else getattr(result, "interrupted", False)
            )
            if interrupted:
                if hasattr(conversation, "stop_tool_execution"):
                    conversation.stop_tool_execution(success=False)
                # Skip adding result - interrupt message is already shown
                continue

            # Gap 7: Special handling for ask_user to display question context
            if tool_name == "ask_user" and isinstance(result, dict):
                self._render_ask_user_stored(conversation, tool_call, result)
                continue

            # Handle spawn_subagent with nested tool calls
            if tool_name == "spawn_subagent":
                nested = getattr(tool_call, "nested_tool_calls", [])
                if nested:
                    self._render_nested_tool_calls(conversation, nested, formatter)
                continue

            # Bash commands special handling
            if tool_name in (
                "bash_execute",
                "run_command",
                "Bash",
            ) and isinstance(result, dict):
                is_error = not result.get("success", True)
                command = parameters.get("command", "")
                working_dir = str(self._working_dir)

                output_parts = []
                if result.get("stdout"):
                    output_parts.append(result["stdout"])
                if result.get("stderr"):
                    output_parts.append(result["stderr"])
                combined_output = "\n".join(output_parts).strip()
                if not combined_output and result.get("output"):
                    output_value = result["output"].strip()
                    if output_value not in (
                        "Command executed",
                        "Command execution failed",
                    ):
                        combined_output = output_value

                if hasattr(conversation, "add_bash_output_box"):
                    conversation.add_bash_output_box(
                        combined_output, is_error, command, working_dir, 0
                    )
                continue

            # All tools: use StyleFormatter
            if isinstance(result, dict):
                formatted = formatter.format_tool_result(tool_name, parameters, result)
                lines = self._extract_result_lines(formatted)
                if lines:
                    conversation.add_tool_result("\n".join(lines))
            else:
                # Fallback for non-dict results
                fallback_lines = self._format_tool_history_lines(tool_call)
                if fallback_lines:
                    conversation.add_tool_result("\n".join(fallback_lines))

    def _render_ask_user_stored(self, conversation: Any, tool_call: Any, result: dict) -> None:
        """Render stored ask_user with question context.

        Args:
            conversation: The conversation widget.
            tool_call: The ask_user tool call object.
            result: The result dictionary containing questions_context and answers.
        """
        questions_context = result.get("questions_context", [])
        answers = result.get("answers", {})
        cancelled = result.get("cancelled", False)

        if questions_context:
            lines = []
            for idx, q in enumerate(questions_context):
                header = q.get("header") or f"Q{idx + 1}"
                question = q.get("question", "")
                answer = answers.get(str(idx), "(not answered)")
                if cancelled:
                    answer = "(cancelled)"
                lines.append(f"[{header}] {question} → {answer}")
            conversation.add_tool_result("\n".join(lines))
        else:
            # Fallback to output text if no questions_context
            output = result.get("output", "")
            if output:
                conversation.add_tool_result(output)

    def _render_nested_tool_calls(
        self, conversation: Any, nested_calls: list[Any], formatter: Any
    ) -> None:
        """Render nested tool calls using existing conversation log methods.

        Args:
            conversation: The conversation widget.
            nested_calls: List of nested tool calls.
            formatter: StyleFormatter instance for formatting results.
        """
        for nested in nested_calls:
            tool_name = getattr(nested, "name", "tool")
            parameters = self._coerce_tool_parameters(getattr(nested, "parameters", {}))
            result = getattr(nested, "result", None)
            approved = getattr(nested, "approved", None)
            depth = 1

            # Convert string result to dict
            if isinstance(result, str):
                result = {"success": True, "output": result}

            # Use unified service for path resolution and header formatting
            display = self._display_service.format_tool_header(tool_name, parameters)

            # Gap 9: Check approval status for nested calls
            if approved is False:
                if hasattr(conversation, "add_nested_tool_call"):
                    conversation.add_nested_tool_call(f"{display} [REJECTED]", depth, "subagent")
                if hasattr(conversation, "add_nested_tool_sub_results"):
                    conversation.add_nested_tool_sub_results(
                        ["⚠ Operation rejected by user"], depth
                    )
                if hasattr(conversation, "complete_nested_tool_call"):
                    conversation.complete_nested_tool_call(tool_name, depth, "subagent", False)
                continue

            if hasattr(conversation, "add_nested_tool_call"):
                conversation.add_nested_tool_call(display, depth, "subagent")

            # Gap 9: Display interrupted nested operations with indicator
            # Note: Don't add "Interrupted by user" message here - it's already shown
            # by ui_callback.on_interrupt() which displays the interrupt message
            # Check for interrupted flag in both dict and dataclass objects (e.g., HttpResult)
            interrupted = (
                result.get("interrupted") if isinstance(result, dict)
                else getattr(result, "interrupted", False)
            )
            if interrupted:
                if hasattr(conversation, "complete_nested_tool_call"):
                    conversation.complete_nested_tool_call(tool_name, depth, "subagent", False)
                continue

            # Display result FIRST, then complete
            success = result.get("success", True) if isinstance(result, dict) else True

            if tool_name in (
                "bash_execute",
                "run_command",
                "Bash",
            ) and isinstance(result, dict):
                # Bash output box (special rendering)
                is_error = not result.get("success", True)
                command = parameters.get("command", "")
                working_dir = parameters.get("working_dir", str(self._working_dir))
                output = result.get("stdout", "") or result.get("output", "")
                if result.get("stderr"):
                    output = (
                        (output + "\n" + result["stderr"]).strip() if output else result["stderr"]
                    )
                if hasattr(conversation, "add_bash_output_box"):
                    conversation.add_bash_output_box(
                        output.strip(),
                        is_error,
                        command,
                        working_dir,
                        depth,
                    )
            elif isinstance(result, dict):
                # All tools: use StyleFormatter
                formatted = formatter.format_tool_result(tool_name, parameters, result)
                lines = self._extract_result_lines(formatted)
                if lines and hasattr(conversation, "add_nested_tool_sub_results"):
                    conversation.add_nested_tool_sub_results(lines, depth)

            # Complete AFTER result display
            if hasattr(conversation, "complete_nested_tool_call"):
                conversation.complete_nested_tool_call(tool_name, depth, "subagent", success)

    @staticmethod
    def _coerce_tool_parameters(raw: Any) -> dict[str, Any]:
        """Ensure tool parameters are a dictionary."""
        if isinstance(raw, dict):
            return raw
        return {}

    def _resolve_paths_for_display(self, params: dict) -> dict:
        """Resolve relative paths to absolute for display.

        Delegates to ToolDisplayService for unified logic.
        """
        return self._display_service.resolve_paths(params)

    def _format_tool_history_lines(self, tool_call: Any) -> list[str]:
        """Convert stored ToolCall data into RichLog-friendly summary lines."""
        lines: list[str] = []
        seen: set[str] = set()

        def add_line(value: str) -> None:
            normalized = value.strip()
            if not normalized or normalized in seen:
                return
            lines.append(normalized)
            seen.add(normalized)

        error = getattr(tool_call, "error", None)
        if error:
            add_line(f"{TOOL_ERROR_SENTINEL} {summarize_error(str(error))}")

        summary = getattr(tool_call, "result_summary", None)
        if summary:
            # Use result_summary
            add_line(str(summary).strip())
        else:
            # Only fall back to truncated raw_result if no summary available
            raw_result = getattr(tool_call, "result", None)
            snippet = self._truncate_tool_output(raw_result)
            if snippet:
                add_line(snippet)

        if not lines:
            add_line("✓ Tool completed")
        return lines

    def _extract_result_lines(self, formatted: str) -> list[str]:
        """Extract result lines from StyleFormatter output.

        Delegates to ToolDisplayService for unified logic.
        """
        return self._display_service.extract_result_lines(formatted)

    def _truncate_tool_output(self, raw_result: Any, max_lines: int = 6, max_chars: int = 400) -> str:
        """Trim long stored tool outputs for concise replay.

        Delegates to ToolDisplayService for unified truncation logic.
        """
        if raw_result is None:
            return ""

        text = str(raw_result).strip()
        if not text:
            return ""

        # Use service's unified truncation with generic mode
        truncated, is_truncated, _ = self._display_service.truncate_output(
            text, mode="generic", head_lines=max_lines, tail_lines=max_chars
        )
        return truncated
