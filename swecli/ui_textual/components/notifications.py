"""Notification center for OpenDev."""

from __future__ import annotations

from collections import deque
from dataclasses import dataclass
from datetime import datetime
from typing import Deque, List, Sequence, Union

from rich.console import Console
from rich.table import Table

from swecli.ui_textual import style_tokens


@dataclass
class Notification:
    """Notification model stored in the center."""

    timestamp: datetime
    level: str
    message: str

    def summary(self) -> str:
        """Return a short human readable summary."""
        time_str = self.timestamp.strftime("%H:%M:%S")
        level_marker = {
            "info": "ℹ",
            "warning": "⚠",
            "error": "✗",
        }.get(self.level, "•")
        return f"{level_marker} {time_str} {self.message}"


class NotificationCenter:
    """Collect and render notifications for the REPL."""

    def __init__(self, console: Console, max_items: int = 50) -> None:
        self.console = console
        self._items: Deque[Notification] = deque(maxlen=max_items)

    def add(self, level: str, message: str) -> Notification:
        """Record a new notification."""
        entry = Notification(timestamp=datetime.now(), level=level, message=message)
        self._items.appendleft(entry)
        return entry

    def latest(self, limit: int = 3) -> List[Notification]:
        """Return newest notifications (head of deque)."""
        return list(list(self._items)[:limit])

    def render(self, limit: Union[int, None] = None) -> None:
        """Render the notification table to the console."""
        if not self._items:
            self.console.print(f"[{style_tokens.SUBTLE}]No notifications yet.[/{style_tokens.SUBTLE}]")
            return

        rows: Sequence[Notification] = self._items if limit is None else list(self._items)[:limit]

        table = Table(title="Notification Center", header_style=f"bold {style_tokens.ACCENT}")
        table.add_column("Time", style=style_tokens.SUBTLE, width=9)
        table.add_column("Level", style="magenta", width=8)
        table.add_column("Message", overflow="fold")

        for notification in rows:
            table.add_row(
                notification.timestamp.strftime("%H:%M:%S"),
                notification.level.capitalize(),
                notification.message,
            )

        self.console.print(table)

    def has_items(self) -> bool:
        """Return True when notifications exist."""
        return bool(self._items)
