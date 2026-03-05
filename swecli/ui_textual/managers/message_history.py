"""Message history management for the Textual chat app."""

from __future__ import annotations

from typing import List


class MessageHistory:
    """Maintain sent-message history and current draft tracking."""

    def __init__(self) -> None:
        self._history: List[str] = []
        self._index: int = -1
        self._draft: str = ""

    def record(self, message: str) -> None:
        """Append a sent message and reset navigation state."""
        self._history.append(message)
        self._index = -1
        self._draft = ""

    def has_history(self) -> bool:
        return bool(self._history)

    def navigate_up(self, current_text: str) -> str | None:
        if not self._history:
            return None

        if self._index == -1:
            self._draft = current_text

        if self._index < len(self._history) - 1:
            self._index += 1

        return self._history[-(self._index + 1)]

    def navigate_down(self) -> str | None:
        if self._index == -1:
            return None

        if self._index > 0:
            self._index -= 1
            return self._history[-(self._index + 1)]

        self._index = -1
        return self._draft

    def reset(self) -> None:
        """Clear draft and reset navigation index without modifying history."""
        self._index = -1
        self._draft = ""

