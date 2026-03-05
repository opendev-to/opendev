"""Console output buffer manager for the Textual chat app."""

from __future__ import annotations

import re
from typing import Optional

from rich.text import Text


class ConsoleBufferManager:
    """Handles buffering and suppression of console output duplicates."""

    def __init__(self, app: "SWECLIChatApp") -> None:
        from typing import TYPE_CHECKING
        if TYPE_CHECKING:  # pragma: no cover
            pass
        self.app = app
        self._buffering = False
        self._queue: list = []

    @property
    def buffering(self) -> bool:
        return self._buffering

    def start(self) -> None:
        self._buffering = True

    def stop(self) -> None:
        self._buffering = False
        self.flush()

    def clear_assistant_history(self) -> None:
        self.app._last_assistant_lines = set()
        self.app._last_rendered_assistant = None
        self.app._last_assistant_normalized = None
        self.app._pending_assistant_normalized = None
        if hasattr(self.app, "_tool_summary"):
            self.app._tool_summary.reset()
        else:
            if hasattr(self.app, "_assistant_response_received"):
                self.app._assistant_response_received = False
            if hasattr(self.app, "_pending_tool_summaries"):
                self.app._pending_tool_summaries.clear()
            if hasattr(self.app, "_saw_tool_result"):
                self.app._saw_tool_result = False

    def last_assistant_message(self) -> Optional[str]:
        return getattr(self.app, "_pending_assistant_normalized", None)

    def enqueue(self, renderable) -> None:
        self._queue.append(renderable)

    def flush(self) -> None:
        if self._buffering or self.app._spinner.active:
            return

        if not self._queue:
            return

        conversation = getattr(self.app, "conversation", None)
        if conversation is None:
            self._queue.clear()
            return

        for renderable in self._queue:
            if not self.should_suppress(renderable):
                conversation.write(renderable)
        self._queue.clear()

    def record_assistant_message(self, message: str) -> None:
        lines = []
        for line in message.splitlines():
            normalized = self._normalize_line(line)
            if normalized:
                lines.append(normalized)
        if not lines:
            normalized = self._normalize_line(message)
            if normalized:
                lines.append(normalized)
        self.app._last_assistant_lines = set(lines)
        self.app._last_rendered_assistant = message.strip()
        self.app._last_assistant_normalized = self._normalize_paragraph(message)
        self.app._pending_assistant_normalized = None
        if hasattr(self.app, "_tool_summary"):
            self.app._tool_summary.on_assistant_message(message)
        else:
            if hasattr(self.app, "_assistant_response_received"):
                self.app._assistant_response_received = True
            if hasattr(self.app, "_pending_tool_summaries"):
                self.app._pending_tool_summaries.clear()
            if hasattr(self.app, "_saw_tool_result"):
                self.app._saw_tool_result = False

    def enqueue_or_write(self, renderable) -> None:
        if self.app._spinner.active or self._buffering:
            self.enqueue(renderable)
            return
        if self.should_suppress(renderable):
            return
        self.app.conversation.write(renderable)

    def clear(self) -> None:
        self._queue.clear()

    def should_suppress(self, renderable) -> bool:
        last_lines = getattr(self.app, "_last_assistant_lines", None) or set()
        if not last_lines:
            return False

        if isinstance(renderable, str):
            segments = [renderable]
        elif isinstance(renderable, Text):
            segments = [renderable.plain]
        elif hasattr(renderable, "render") and hasattr(self.app, "console"):
            try:
                console = self.app.console
                segments = [
                    segment.text
                    for segment in console.render(renderable)
                    if getattr(segment, "text", "")
                ]
            except Exception:  # pragma: no cover
                return False
        else:
            return False

        combined = " ".join(segments)
        normalized_combined = self._normalize_paragraph(combined)
        targets = [
            value
            for value in (
                getattr(self.app, "_pending_assistant_normalized", None),
                getattr(self.app, "_last_assistant_normalized", None),
            )
            if value
        ]
        if normalized_combined and targets:
            if any(normalized_combined == target for target in targets):
                return True

        normalized_segments = [self._normalize_line(seg) for seg in segments]
        normalized_segments = [seg for seg in normalized_segments if seg]
        if normalized_segments and all(seg in last_lines for seg in normalized_segments):
            return True

        return False

    @staticmethod
    def _normalize_line(line: str) -> str:
        cleaned = re.sub(r"\x1b\[[0-9;]*m", "", line)
        cleaned = cleaned.strip()
        if not cleaned:
            return ""
        if cleaned.startswith("⏺"):
            cleaned = cleaned.lstrip("⏺").strip()
        cleaned = re.sub(r"\s+", " ", cleaned)
        return cleaned

    @staticmethod
    def _normalize_paragraph(text: str) -> str:
        cleaned = re.sub(r"\x1b\[[0-9;]*m", "", text)
        cleaned = cleaned.replace("⏺", " ")
        cleaned = cleaned.replace("\n", " ")
        cleaned = re.sub(r"\s+", " ", cleaned)
        return cleaned.strip()


__all__ = ["ConsoleBufferManager"]
