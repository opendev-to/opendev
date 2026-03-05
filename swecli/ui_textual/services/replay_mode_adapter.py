"""Replay mode adapter for static tool display during session replay."""

from __future__ import annotations

from typing import Any

from rich.text import Text

from swecli.ui_textual.services.display_data import ToolResultData


class ReplayModeAdapter:
    """Adapter for session replay with static display.

    This adapter renders tool calls and results without spinners or animations,
    suitable for replaying persisted session history.

    Attributes:
        conversation: The ConversationLog widget to render into.
    """

    def __init__(self, conversation: Any):
        """Initialize the ReplayModeAdapter.

        Args:
            conversation: The ConversationLog widget to render into.
        """
        self._conversation = conversation

    def render_tool_call(
        self,
        header: Text,
        tool_call_id: str,
        is_rejected: bool = False,
    ) -> None:
        """Render a tool call header (static, no spinner).

        Args:
            header: Rich Text with formatted tool call header.
            tool_call_id: Unique ID for this tool call (for tracking).
            is_rejected: If True, append [REJECTED] to header.
        """
        if is_rejected:
            # Append rejection marker to header
            if hasattr(self._conversation, "add_tool_call"):
                self._conversation.add_tool_call(f"{header} [REJECTED]")
        else:
            if hasattr(self._conversation, "add_tool_call"):
                self._conversation.add_tool_call(header)

        # Immediately mark as complete (no spinner animation)
        if hasattr(self._conversation, "stop_tool_execution"):
            self._conversation.stop_tool_execution(success=not is_rejected)

    def render_tool_result(
        self,
        result: ToolResultData,
        tool_call_id: str,
        tool_name: str = "",
    ) -> None:
        """Render a tool result (static display).

        Args:
            result: The ToolResultData containing formatted result.
            tool_call_id: Unique ID of the tool call.
            tool_name: Name of the tool (for special handling).
        """
        # Handle special states first
        # Note: Don't add "Interrupted by user" message here - it's already shown
        # by the interrupt handler (ui_callback.on_interrupt())
        if result.is_interrupted:
            return

        if result.is_rejected:
            if hasattr(self._conversation, "add_tool_result"):
                self._conversation.add_tool_result("\u26a0 Operation rejected by user")
            return

        # Handle special types
        if result.special_type == "bash" and result.bash_data:
            self._render_bash_output(result, depth=0)
        else:
            # Render result lines, defaulting to "Completed" when empty
            combined = "\n".join(result.lines) if result.lines else "Completed"
            if hasattr(self._conversation, "add_tool_result"):
                self._conversation.add_tool_result(combined)

    def render_nested_tool_call(
        self,
        header: Text,
        depth: int,
        parent: str,
        tool_call_id: str = "",
        is_rejected: bool = False,
    ) -> None:
        """Render a nested tool call header.

        Args:
            header: Rich Text with formatted tool call header.
            depth: Nesting depth level.
            parent: Name/identifier of the parent subagent.
            tool_call_id: Unique ID for this tool call.
            is_rejected: If True, append [REJECTED] to header.
        """
        if is_rejected:
            if hasattr(self._conversation, "add_nested_tool_call"):
                self._conversation.add_nested_tool_call(
                    f"{header} [REJECTED]", depth, parent, tool_call_id
                )
        else:
            if hasattr(self._conversation, "add_nested_tool_call"):
                self._conversation.add_nested_tool_call(header, depth, parent, tool_call_id)

    def render_nested_tool_result(
        self,
        result: ToolResultData,
        depth: int,
        parent: str,
        tool_name: str = "",
        tool_call_id: str = "",
    ) -> None:
        """Render a nested tool result.

        Args:
            result: The ToolResultData containing formatted result.
            depth: Nesting depth level.
            parent: Name/identifier of the parent subagent.
            tool_name: Name of the tool.
            tool_call_id: Unique ID of the tool call.
        """
        success = result.success

        # Handle special states
        # Note: Don't add "Interrupted by user" message here - it's already shown
        # by the interrupt handler (ui_callback.on_interrupt())
        if result.is_interrupted:
            if hasattr(self._conversation, "complete_nested_tool_call"):
                self._conversation.complete_nested_tool_call(
                    tool_name, depth, parent, False, tool_call_id
                )
            return

        if result.is_rejected:
            if hasattr(self._conversation, "add_nested_tool_sub_results"):
                self._conversation.add_nested_tool_sub_results(
                    ["\u26a0 Operation rejected by user"], depth
                )
            if hasattr(self._conversation, "complete_nested_tool_call"):
                self._conversation.complete_nested_tool_call(
                    tool_name, depth, parent, False, tool_call_id
                )
            return

        # Handle bash output specially
        if result.special_type == "bash" and result.bash_data:
            self._render_bash_output(result, depth)
        elif result.lines:
            if hasattr(self._conversation, "add_nested_tool_sub_results"):
                self._conversation.add_nested_tool_sub_results(result.lines, depth)

        # Mark nested call as complete
        if hasattr(self._conversation, "complete_nested_tool_call"):
            self._conversation.complete_nested_tool_call(
                tool_name, depth, parent, success, tool_call_id
            )

    def _render_bash_output(self, result: ToolResultData, depth: int = 0) -> None:
        """Render bash command output.

        Args:
            result: The ToolResultData with bash_data.
            depth: Nesting depth (0 for main agent, 1+ for nested).
        """
        if not result.bash_data:
            return

        # Defense-in-depth: ensure non-empty output for display
        output = result.bash_data.output
        if not output:
            output = "Command failed" if result.bash_data.is_error else "Completed"

        if depth == 0:
            # Main agent bash output
            if hasattr(self._conversation, "add_bash_output_box"):
                self._conversation.add_bash_output_box(
                    output,
                    result.bash_data.is_error,
                    result.bash_data.command,
                    result.bash_data.working_dir,
                    depth,
                )
        else:
            # Nested bash output
            if hasattr(self._conversation, "add_bash_output_box"):
                self._conversation.add_bash_output_box(
                    output,
                    result.bash_data.is_error,
                    result.bash_data.command,
                    result.bash_data.working_dir,
                    depth,
                )
