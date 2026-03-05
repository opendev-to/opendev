"""Live mode adapter for tool display with spinner/animation support."""

from __future__ import annotations

from typing import Any, Dict, Optional

from rich.text import Text

from swecli.ui_textual.services.display_data import ToolResultData
from swecli.ui_textual.style_tokens import GREY


class LiveModeAdapter:
    """Adapter for live execution with animations and spinners.

    This adapter wraps the conversation log and spinner service to provide
    real-time tool display with progress indicators during live execution.

    Attributes:
        conversation: The ConversationLog widget to render into.
        spinner_service: The SpinnerService for managing progress spinners.
    """

    def __init__(self, conversation: Any, spinner_service: Optional[Any] = None):
        """Initialize the LiveModeAdapter.

        Args:
            conversation: The ConversationLog widget to render into.
            spinner_service: Optional SpinnerService for spinner management.
        """
        self._conversation = conversation
        self._spinner_service = spinner_service
        # Track active spinners by tool_call_id
        self._active_spinners: Dict[str, str] = {}

    def render_tool_call(
        self,
        header: Text,
        tool_call_id: str,
        tool_name: str = "",
        skip_placeholder: bool = False,
    ) -> None:
        """Render a tool call with spinner animation.

        Args:
            header: Rich Text with formatted tool call header.
            tool_call_id: Unique ID for tracking this tool call.
            tool_name: Name of the tool (used to determine if placeholder needed).
            skip_placeholder: If True, skip adding placeholder for result.
        """
        # Determine if this is a bash command (no placeholder needed)
        is_bash = tool_name in ("bash_execute", "run_command", "Bash")

        if self._spinner_service is not None:
            spinner_id = self._spinner_service.start(
                header, skip_placeholder=skip_placeholder or is_bash
            )
            self._active_spinners[tool_call_id] = spinner_id
        else:
            # Fallback to direct conversation calls
            if hasattr(self._conversation, "add_tool_call"):
                self._conversation.add_tool_call(header)
            if hasattr(self._conversation, "start_tool_execution"):
                self._conversation.start_tool_execution()

    def render_tool_result(
        self,
        result: ToolResultData,
        tool_call_id: str,
        tool_name: str = "",
    ) -> None:
        """Render a tool result and stop the spinner.

        Args:
            result: The ToolResultData containing formatted result.
            tool_call_id: Unique ID of the tool call.
            tool_name: Name of the tool (for special handling).
        """
        # Stop spinner if active
        spinner_id = self._active_spinners.pop(tool_call_id, None)

        # Get first result line for spinner message
        first_line = result.lines[0] if result.lines else "Completed"

        if spinner_id and self._spinner_service is not None:
            self._spinner_service.stop(spinner_id, result.success, first_line)
        elif hasattr(self._conversation, "stop_tool_execution"):
            self._conversation.stop_tool_execution(result.success)

        # Handle special types
        if result.special_type == "bash" and result.bash_data:
            self._render_bash_output(result)
        elif result.is_interrupted:
            # Don't add duplicate "Interrupted by user" message here
            # The interrupt message is already shown by ui_callback.on_interrupt()
            pass
        elif result.is_rejected:
            self._conversation.add_tool_result("Operation rejected by user")
        elif result.lines:
            # Skip first line if it was already shown in spinner
            remaining_lines = result.lines[1:] if first_line else result.lines
            for line in remaining_lines:
                self._conversation.add_tool_result(line)

            # Add continuation lines (e.g., diff content)
            if result.continuation_lines:
                if hasattr(self._conversation, "add_tool_result_continuation"):
                    self._conversation.add_tool_result_continuation(result.continuation_lines)

    def render_tool_result_line(self, line: str) -> None:
        """Render a single tool result line.

        Args:
            line: The result line to display.
        """
        result_line = Text("  \u23bf  ", style=GREY)
        result_line.append(line, style=GREY)
        if hasattr(self._conversation, "write"):
            self._conversation.write(result_line)
        elif hasattr(self._conversation, "add_tool_result"):
            self._conversation.add_tool_result(line)

    def _render_bash_output(self, result: ToolResultData) -> None:
        """Render bash command output in VS Code Terminal style.

        Args:
            result: The ToolResultData with bash_data.
        """
        if not result.bash_data:
            return

        # Defense-in-depth: ensure non-empty output for display
        output = result.bash_data.output
        if not output:
            output = "Command failed" if result.bash_data.is_error else "Completed"

        if hasattr(self._conversation, "add_bash_output_box"):
            self._conversation.add_bash_output_box(
                output,
                result.bash_data.is_error,
                result.bash_data.command,
                result.bash_data.working_dir,
                0,  # depth for main agent
            )

    def stop_all_spinners(self, success: bool = False) -> None:
        """Stop all active spinners (e.g., on interrupt).

        Args:
            success: Whether to mark spinners as successful.
        """
        if self._spinner_service is not None:
            for spinner_id in self._active_spinners.values():
                self._spinner_service.stop(spinner_id, success)
        self._active_spinners.clear()
