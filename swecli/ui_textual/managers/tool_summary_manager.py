"""Tool summary manager for fallback assistant messaging."""

from __future__ import annotations

from typing import TYPE_CHECKING

from swecli.ui_textual.utils.tool_display import get_tool_display_parts, summarize_tool_arguments


class ToolSummaryManager:
    """Handles tool summary recording and inline follow-up messages."""

    def __init__(self, app: "SWECLIChatApp") -> None:
        if TYPE_CHECKING:  # pragma: no cover
            pass
        self.app = app
        self.reset()

    def reset(self) -> None:
        self._pending: list[str] = []
        self._saw_tool_result = False
        self._assistant_response_received = False

    def record_summary(
        self,
        tool_name: str,
        tool_args: dict[str, object],
        result_lines: list[str],
    ) -> None:
        if not result_lines:
            return

        summary = self._build_summary(tool_name, tool_args, result_lines)
        if not summary:
            return

        self._pending.append(summary)
        self._saw_tool_result = True

    def on_assistant_message(self, message: str) -> None:
        self._assistant_response_received = True
        self._pending.clear()
        self._saw_tool_result = False

    def emit_follow_up_if_needed(self) -> None:
        conversation = getattr(self.app, "conversation", None)
        if conversation is None:
            self.reset()
            return

        if self._assistant_response_received or not self._saw_tool_result:
            self.reset()
            return

        if not self._pending:
            self._saw_tool_result = False
            return

        if len(self._pending) == 1:
            message = self._pending[0]
        else:
            lines = ["Summary of tool activity:"]
            lines.extend(f"- {summary}" for summary in self._pending)
            message = "\n".join(lines)

        if hasattr(self.app, "_stop_local_spinner"):
            self.app._stop_local_spinner()
        conversation.add_assistant_message(message)
        if hasattr(self.app, "record_assistant_message"):
            self.app.record_assistant_message(message)
        self.reset()

    def _build_summary(
        self,
        tool_name: str,
        tool_args: dict[str, object],
        result_lines: list[str],
    ) -> str:
        primary = (result_lines[0] or "").strip()
        if not primary:
            return ""

        verb, label = get_tool_display_parts(tool_name)
        friendly_tool = f"{verb}({label})" if label else verb
        summary = summarize_tool_arguments(tool_name, tool_args)

        if not primary.endswith((".", "!", "?")):
            primary = f"{primary}."

        if primary and primary[0].islower():
            primary = primary[0].upper() + primary[1:]

        prefix = f"{friendly_tool} ({summary})" if summary else friendly_tool

        if len(result_lines) > 1:
            return f"Completed {prefix} — {primary}"
        return f"Completed {prefix}."


__all__ = ["ToolSummaryManager"]
