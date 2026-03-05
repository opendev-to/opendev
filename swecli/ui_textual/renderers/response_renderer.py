"""Response and tool call rendering for the Textual chat app."""

from __future__ import annotations

from typing import Any, TYPE_CHECKING

from swecli.models.message import ChatMessage, Role
from swecli.ui_textual.constants import TOOL_ERROR_SENTINEL
from swecli.ui_textual.utils import build_tool_call_text
from swecli.ui_textual.utils.text_utils import truncate_tool_output, summarize_error

if TYPE_CHECKING:  # pragma: no cover
    from swecli.ui_textual.chat_app import SWECLIChatApp


class ResponseRenderer:
    """Handle rendering of assistant responses and tool calls."""

    def __init__(self, app: "SWECLIChatApp") -> None:
        """Initialize the response renderer.

        Args:
            app: The Textual chat application
        """
        self.app = app
        self._last_assistant_message: str | None = None
        self._last_assistant_message_normalized: str | None = None
        self._suppress_console_duplicate = False

    def render_responses(self, messages: list[ChatMessage]) -> None:
        """Render new session messages inside the Textual conversation log.

        Args:
            messages: List of chat messages to render
        """
        buffer_started = False
        assistant_text_rendered = False

        for msg in messages:
            if msg.role == Role.ASSISTANT:
                if hasattr(self.app, "_stop_local_spinner"):
                    self.app._stop_local_spinner()

                if hasattr(self.app, "start_console_buffer"):
                    self.app.start_console_buffer()
                    buffer_started = True

                content = msg.content.strip()
                if hasattr(self.app, "_normalize_paragraph"):
                    normalized = self.app._normalize_paragraph(content)
                    if normalized:
                        self.app._pending_assistant_normalized = normalized
                        self._last_assistant_message_normalized = normalized
                else:
                    self._last_assistant_message_normalized = content if content else None

                # Only render assistant messages that DON'T have tool calls
                # Messages with tool calls were already displayed in real-time by callbacks
                has_tool_calls = getattr(msg, "tool_calls", None) and len(msg.tool_calls) > 0

                if content and not has_tool_calls:
                    self.app.conversation.add_assistant_message(msg.content)
                    if hasattr(self.app, "record_assistant_message"):
                        self.app.record_assistant_message(msg.content)
                    if hasattr(self.app, "_last_rendered_assistant"):
                        self.app._last_rendered_assistant = content
                    self._last_assistant_message = content
                    self._suppress_console_duplicate = True
                    assistant_text_rendered = True

                # Skip rendering messages with tool calls - already shown in real-time
            elif msg.role == Role.SYSTEM:
                self.app.conversation.add_system_message(msg.content)
            # Skip USER messages - they're already displayed by the UI when user types them

        if buffer_started and hasattr(self.app, "stop_console_buffer"):
            self.app.stop_console_buffer()

    def render_stored_tool_calls(self, conversation, tool_calls: list[Any]) -> None:
        """Replay historical tool calls and results.

        Args:
            conversation: The conversation widget
            tool_calls: List of tool calls to render
        """
        if not tool_calls:
            return

        for tool_call in tool_calls:
            try:
                parameters = self._coerce_tool_parameters(getattr(tool_call, "parameters", {}))
            except Exception:
                parameters = {}

            display = build_tool_call_text(getattr(tool_call, "name", "tool"), parameters)
            conversation.add_tool_call(display)
            if hasattr(conversation, "stop_tool_execution"):
                conversation.stop_tool_execution()

            lines = self._format_tool_history_lines(tool_call)
            if lines:
                conversation.add_tool_result("\n".join(lines))

    @staticmethod
    def _coerce_tool_parameters(raw: Any) -> dict[str, Any]:
        """Coerce raw parameters to dictionary format.

        Args:
            raw: Raw parameter data

        Returns:
            Dictionary of parameters
        """
        if isinstance(raw, dict):
            return raw
        return {}

    def _format_tool_history_lines(self, tool_call: Any) -> list[str]:
        """Convert stored ToolCall data into RichLog-friendly summary lines.

        Args:
            tool_call: The tool call object

        Returns:
            List of formatted lines
        """
        lines: list[str] = []
        seen: set[str] = set()

        def add_line(value: str) -> None:
            normalized = value.strip()
            if not normalized or normalized in seen:
                return
            lines.append(normalized)
            seen.add(normalized)

        # Skip interrupted operations - already handled by interrupt display
        if getattr(tool_call, "interrupted", False):
            return lines

        error = getattr(tool_call, "error", None)
        if error:
            add_line(f"{TOOL_ERROR_SENTINEL} {summarize_error(str(error))}")

        summary = getattr(tool_call, "result_summary", None)
        if summary:
            add_line(str(summary).strip())

        raw_result = getattr(tool_call, "result", None)
        snippet = truncate_tool_output(raw_result)
        if snippet:
            add_line(snippet)

        if not lines:
            add_line("✓ Tool completed")
        return lines


__all__ = ["ResponseRenderer"]
