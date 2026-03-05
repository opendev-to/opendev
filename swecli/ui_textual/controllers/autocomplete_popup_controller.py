"""Autocomplete popup controller for the Textual chat app."""

from __future__ import annotations

from typing import Optional, Tuple

from rich.console import Group
from rich.table import Table
from rich.text import Text
from textual.widgets import Static

from swecli.ui_textual.style_tokens import BLUE_BG_ACTIVE, GREY
from swecli.ui_textual.utils.file_type_colors import FileTypeColors

StateType = Tuple[Tuple[Tuple[str, str], ...], int]


class AutocompletePopupController:
    """Encapsulates autocomplete popup rendering and deduplication."""

    def __init__(self, popup: Static) -> None:
        self.popup = popup
        self._last_state: StateType | None = None

    def reset(self) -> None:
        """Hide the popup and clear cached state."""
        self._last_state = None
        self.hide()

    def hide(self) -> None:
        """Hide the popup without clearing cached state."""
        self.popup.update("")
        self.popup.styles.display = "none"

    def render(
        self,
        entries: list[tuple[str, str]],
        selected_index: Optional[int] = None,
    ) -> None:
        """Render autocomplete suggestions."""

        if not entries:
            self.reset()
            return

        total = len(entries)
        limit = min(total, 5)
        active = selected_index if selected_index is not None else 0
        active = max(0, min(active, total - 1))

        window_start = 0
        if total > limit:
            window_start = max(0, active - limit + 1)
            window_start = min(window_start, total - limit)
        window_end = window_start + limit

        rows = [
            (label or "", meta or "")
            for label, meta in entries[window_start:window_end]
        ]

        window_active = active - window_start
        # Ensure window_active is within valid range for the rows array
        window_active = max(0, min(window_active, len(rows) - 1)) if rows else 0
        state: StateType = (tuple(rows), window_active)

        # Always re-render to ensure highlight follows selection
        # The state caching was preventing highlight updates when only
        # the selection index changed within the same visible window

        table = Table.grid(padding=(0, 1))
        table.expand = True
        table.add_column(justify="left", width=2, no_wrap=True)
        table.add_column(justify="left", ratio=3, overflow="fold")
        table.add_column(justify="right", ratio=2, no_wrap=True)

        header_text = Text("Mention a file · Enter to insert\n", style=f"bold {GREY}")

        for index, (label, meta) in enumerate(rows):
            is_active = index == window_active
            file_color = FileTypeColors.get_color_from_icon_label(label)

            pointer_char = "▸" if is_active else "•"
            pointer_style = f"bold {file_color}" if is_active else f"dim {GREY}"
            pointer_render = Text(pointer_char, style=pointer_style)

            label_style = f"bold {file_color}" if is_active else file_color
            label_render = Text(label, style=label_style, overflow="ellipsis")

            meta_style = f"bold {GREY}" if is_active else GREY
            meta_render = Text(meta, style=meta_style, overflow="ellipsis")

            row_style = f"on {BLUE_BG_ACTIVE}" if is_active else None
            table.add_row(pointer_render, label_render, meta_render, style=row_style)

        renderable = Group(header_text, table)

        self.popup.update(renderable)
        self.popup.styles.display = "block"
        self._last_state = state
        # Force refresh to ensure visual update
        self.popup.refresh()
