from __future__ import annotations

import re
import threading
import time
from typing import Any

from rich.console import Console
from rich.text import Text
from textual.geometry import Size
from textual.strip import Strip

from swecli.ui_textual.style_tokens import GREY
from swecli.ui_textual.widgets.conversation.protocols import RichLogInterface

from textual.strip import Strip

from swecli.ui_textual.style_tokens import GREY, BLUE_BRIGHT
from swecli.ui_textual.widgets.conversation.protocols import RichLogInterface


class DefaultSpinnerManager:
    """Handles the 'thinking' spinner animation."""

    MIN_VISIBLE_MS = 300

    def __init__(self, log: RichLogInterface, app_callback_interface: Any = None):
        self.log = log
        self.app = app_callback_interface

        # State
        self._spinner_start: int | None = None
        self._spinner_line_count = 0
        self._spinner_active = False
        self._spinner_index = 0
        self._spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        self._thinking_message = ""
        self._thinking_tip = ""
        self._thinking_started_at = 0.0

        # Timers
        self._spinner_timer: Any | None = None  # Textual timer
        self._thread_timer: threading.Timer | None = None
        self._pending_stop_timer: Any | None = None

        self._last_tick_time = 0.0

        # Resize coordination
        self._paused_for_resize = False

    def cleanup(self) -> None:
        """Stop all timers."""
        if self._spinner_timer is not None:
            self._spinner_timer.stop()
            self._spinner_timer = None
        if self._thread_timer is not None:
            self._thread_timer.cancel()
            self._thread_timer = None
        if self._pending_stop_timer is not None:
            self._pending_stop_timer.stop()
            self._pending_stop_timer = None

    # --- Resize Coordination Methods ---

    def pause_for_resize(self) -> None:
        """Stop animation timers for resize."""
        self._paused_for_resize = True
        if self._spinner_timer is not None:
            self._spinner_timer.stop()
            self._spinner_timer = None
        if self._thread_timer is not None:
            self._thread_timer.cancel()
            self._thread_timer = None

    def adjust_indices(self, delta: int, first_affected: int) -> None:
        """Shift spinner line index by delta if affected.

        Args:
            delta: Number of lines added (positive) or removed (negative)
            first_affected: First line index affected by the change
        """
        if self._spinner_start is not None and self._spinner_start >= first_affected:
            self._spinner_start += delta

    def resume_after_resize(self) -> None:
        """Restart animation after resize."""
        self._paused_for_resize = False
        if self._spinner_active and self._spinner_start is not None:
            self._render_thinking_spinner_frame()
            self._schedule_thinking_spinner()

    def start_spinner(self, message: Text | str) -> None:
        """Append spinner output at the end of the log and start animation."""
        if self._pending_stop_timer is not None:
            self._pending_stop_timer.stop()
            self._pending_stop_timer = None
            # Clean up old spinner and reset state for fresh start with new tip
            self._remove_spinner_lines()
            self._spinner_start = None
            self._spinner_line_count = 0
            self._spinner_active = False

        if self._spinner_active and self._spinner_start is not None:
            self.update_spinner(message)
            return

        # Ensure we are appending to the end
        self._spinner_start = len(self.log.lines)
        self._parse_message(message)

        self._spinner_active = True
        self._spinner_index = 0
        self._thinking_started_at = time.monotonic()
        self._last_tick_time = self._thinking_started_at

        self._render_thinking_spinner_frame()
        self._schedule_thinking_spinner()

    def update_spinner(self, message: Text | str) -> None:
        """Update spinner message without restarting animation."""
        if self._spinner_start is None:
            self.start_spinner(message)
            return

        self._parse_message(message)
        self._render_thinking_spinner_frame()

    def stop_spinner(self) -> None:
        """Request spinner removal (may be delayed for minimum visibility)."""
        if self._pending_stop_timer is not None:
            self._pending_stop_timer.stop()
            self._pending_stop_timer = None

        if not self._spinner_active or self._spinner_start is None:
            return

        elapsed_ms = (time.monotonic() - self._thinking_started_at) * 1000
        remaining_ms = max(0, self.MIN_VISIBLE_MS - elapsed_ms)

        if remaining_ms > 0:
            self._pending_stop_timer = self.log.set_timer(
                remaining_ms / 1000, self._do_stop_spinner
            )
        else:
            self._do_stop_spinner()

    def tick_spinner(self) -> None:
        """Called externally to advance spinner animation during streaming."""
        if not self._spinner_active or self._spinner_start is None:
            return

        now = time.monotonic()
        if now - self._last_tick_time < 0.1:
            return
        self._last_tick_time = now
        self._advance_spinner_frame()

    # --- Private Helpers ---

    def _parse_message(self, message: Text | str) -> None:
        if isinstance(message, Text):
            plain = message.plain
            if "\n" in plain:
                parts = plain.split("\n", 1)
                self._thinking_message = parts[0].strip()
                if len(parts) > 1 and "Tip:" in parts[1]:
                    tip_match = re.search(r"Tip:\s*(.+)", parts[1])
                    if tip_match:
                        self._thinking_tip = tip_match.group(1).strip()
                    else:
                        self._thinking_tip = ""
                else:
                    self._thinking_tip = ""
            else:
                self._thinking_message = plain.strip()
                self._thinking_tip = ""
        else:
            self._thinking_message = str(message)
            self._thinking_tip = ""

    def _do_stop_spinner(self) -> None:
        self._pending_stop_timer = None

        if self._spinner_timer is not None:
            self._spinner_timer.stop()
            self._spinner_timer = None
        if self._thread_timer is not None:
            self._thread_timer.cancel()
            self._thread_timer = None

        self._spinner_active = False

        if self._spinner_start is not None:
            self._remove_spinner_lines()
            self._spinner_start = None
            self._spinner_line_count = 0

        self._thinking_message = ""
        self._thinking_tip = ""
        self._thinking_started_at = 0.0

    def _schedule_thinking_spinner(self) -> None:
        if self._spinner_timer is not None:
            self._spinner_timer.stop()
        if self._thread_timer is not None:
            self._thread_timer.cancel()
            self._thread_timer = None

        self._spinner_timer = self.log.set_timer(0.12, self._animate_thinking_spinner)

        self._thread_timer = threading.Timer(0.12, self._thread_animate_spinner)
        self._thread_timer.daemon = True
        self._thread_timer.start()

    def _thread_animate_spinner(self) -> None:
        if not self._spinner_active:
            return
        if self.app is not None and hasattr(self.app, "call_from_thread"):
            try:
                self.app.call_from_thread(self._animate_thinking_spinner)
            except Exception:
                pass

    def _animate_thinking_spinner(self) -> None:
        if not self._spinner_active:
            return
        self._advance_spinner_frame()
        self._schedule_thinking_spinner()

    def _advance_spinner_frame(self) -> None:
        if not self._spinner_active:
            return
        self._spinner_index = (self._spinner_index + 1) % len(self._spinner_chars)
        self._render_thinking_spinner_frame()

    def _render_thinking_spinner_frame(self) -> None:
        if self._paused_for_resize:
            return  # Skip render during resize
        if self._spinner_start is None:
            return

        elapsed = 0
        if self._thinking_started_at:
            elapsed = int(time.monotonic() - self._thinking_started_at)

        frame = self._spinner_chars[self._spinner_index]
        suffix = f" ({elapsed}s)"

        renderable = Text()
        renderable.append(frame, style=BLUE_BRIGHT)
        renderable.append(f" {self._thinking_message}{suffix}", style=BLUE_BRIGHT)

        if self._spinner_line_count == 0:
            # Initial render
            self.log.write(renderable, scroll_end=True, animate=False)
            if self._thinking_tip:
                tip_line = Text()
                tip_line.append("  ⎿  Tip: ", style=GREY)
                tip_line.append(self._thinking_tip, style=GREY)
                self.log.write(tip_line, scroll_end=True, animate=False)

            # Recalculate based on what was written
            # Note: RichLog.write may be async, so len(lines) might not update immediately.
            # We enforce a minimum count to prevent repeat initial renders.
            current_len = len(self.log.lines)
            calculated_count = current_len - self._spinner_start

            if calculated_count <= 0:
                # Fallback if write hasn't reflected in lines yet
                self._spinner_line_count = 2 if self._thinking_tip else 1
            else:
                self._spinner_line_count = calculated_count

        else:
            # Update animation frame
            if self._spinner_start >= len(self.log.lines):
                # Lost sync (e.g. user cleared log), try to resync or stop
                self._spinner_active = False
                return

            from rich.console import Console

            console = Console(width=1000, force_terminal=True, no_color=False)
            segments = list(renderable.render(console))
            strip = Strip(segments)

            # STRATEGY: Delete & Insert to force RichLog/Textual update
            try:
                if self._spinner_start < len(self.log.lines):
                    del self.log.lines[self._spinner_start]
                    self.log.lines.insert(self._spinner_start, strip)

                # Try to call refresh_line if available (cache invalidation)
                if hasattr(self.log, "refresh_line"):
                    self.log.refresh_line(self._spinner_start)
                else:
                    self.log.refresh()
            except Exception:
                # Basic fallback if list access fails
                pass

            if self.app is not None and hasattr(self.app, "refresh"):
                self.app.refresh()

    def _remove_spinner_lines(self) -> None:
        if self._spinner_start is None:
            return

        start = min(self._spinner_start, len(self.log.lines))
        end = min(start + self._spinner_line_count, len(self.log.lines))

        if start < end:
            # Only delete spinner lines, not content added after
            protected_lines = getattr(self.log, "_protected_lines", set())

            to_delete = [i for i in range(start, end) if i not in protected_lines]
            for i in sorted(to_delete, reverse=True):
                if i < len(self.log.lines):
                    del self.log.lines[i]

            # Sync block registry so stale blocks don't re-render on resize
            actual_deleted = len(to_delete)
            if actual_deleted > 0 and hasattr(self.log, "_block_registry"):
                self.log._block_registry.remove_lines_range(start, actual_deleted)

            # Update protected line indices
            if protected_lines:
                new_protected = set()
                for p in protected_lines:
                    if p < start:
                        new_protected.add(p)
                    else:
                        deleted_before = len([i for i in to_delete if i < p])
                        new_protected.add(p - deleted_before)

                if hasattr(self.log, "_protected_lines"):
                    self.log._protected_lines.clear()
                    self.log._protected_lines.update(new_protected)

            # Force virtual size update logic from original log
            if hasattr(self.log, "_line_cache"):
                self.log._line_cache.clear()

            # Refresh display after line removal — critical for the deferred
            # stop path where no subsequent write triggers a repaint.
            self.log.refresh()
