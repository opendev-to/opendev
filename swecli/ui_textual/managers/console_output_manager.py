"""Console output management for the Textual chat app."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from rich.ansi import AnsiDecoder
from rich.text import Text

from swecli.ui_textual.utils.text_utils import is_spinner_text, is_spinner_tip, normalize_console_text

if TYPE_CHECKING:  # pragma: no cover
    from swecli.ui_textual.chat_app import SWECLIChatApp


class ConsoleOutputManager:
    """Manage console output rendering and bridging."""

    def __init__(self, app: "SWECLIChatApp", console_queue: asyncio.Queue, loop: asyncio.AbstractEventLoop) -> None:
        """Initialize the console output manager.

        Args:
            app: The Textual chat application
            console_queue: Queue for console text
            loop: The event loop
        """
        self.app = app
        self._console_queue = console_queue
        self._loop = loop
        self._ansi_decoder = AnsiDecoder()
        self._last_console_line: str | None = None
        self._suppress_console_duplicate = False
        self._last_assistant_message_normalized: str | None = None
        self._original_console_print = None
        self._original_console_log = None

    def install_console_bridge(self, console) -> None:
        """Mirror console prints/logs into the Textual conversation.

        Args:
            console: The Rich console to bridge
        """
        self._original_console_print = getattr(console, "print")
        self._original_console_log = getattr(console, "log", None)

        def bridge_print(*args, **kwargs):
            with console.capture() as capture:
                self._original_console_print(*args, **kwargs)
            text = capture.get()
            if text.strip():
                self.enqueue_console_text(text)

        console.print = bridge_print  # type: ignore[assignment]

        if self._original_console_log is not None:
            def bridge_log(*args, **kwargs):
                with console.capture() as capture:
                    self._original_console_log(*args, **kwargs)
                text = capture.get()
                if text.strip():
                    self.enqueue_console_text(text)

            console.log = bridge_log  # type: ignore[assignment]

    def enqueue_console_text(self, text: str) -> None:
        """Enqueue console text for rendering.

        Args:
            text: Console text to enqueue
        """
        if not text:
            return

        if hasattr(self.app, "_normalize_paragraph"):
            normalized = self.app._normalize_paragraph(text)
            last_assistant = getattr(self.app, "_last_assistant_normalized", None)
            if normalized and last_assistant and normalized == last_assistant:
                return

        try:
            running_loop = asyncio.get_running_loop()
        except RuntimeError:
            running_loop = None

        if running_loop is self._loop:
            self._console_queue.put_nowait(text)
        elif self._loop.is_running():
            self._loop.call_soon_threadsafe(self._console_queue.put_nowait, text)
        else:
            self._console_queue.put_nowait(text)

    async def drain_console_queue(self) -> None:
        """Drain the console queue and render output."""
        try:
            while True:
                text = await self._console_queue.get()
                self.render_console_output(text)
                self._console_queue.task_done()
        except asyncio.CancelledError:  # pragma: no cover - task shutdown
            return

    def render_console_output(self, text: str) -> None:
        """Render console output captured from REPL commands/processings.

        Args:
            text: Console text to render
        """
        normalized = normalize_console_text(text)
        renderables = list(self._ansi_decoder.decode(normalized))
        if not renderables:
            return

        # Remove the blank line that was added after user message
        # so error appears directly under the command
        if hasattr(self.app, "conversation") and len(self.app.conversation.lines) > 0:
            last_line = self.app.conversation.lines[-1]
            if hasattr(last_line, 'plain') and not last_line.plain.strip():
                self.app.conversation.lines.pop()

        has_rendered_content = False
        for renderable in renderables:
            if isinstance(renderable, Text):
                plain = renderable.plain.strip()
                if not plain:
                    continue
                if is_spinner_text(plain) or is_spinner_tip(plain):
                    continue
                normalized_plain = plain.strip()
                if hasattr(self.app, "_normalize_paragraph"):
                    normalized_plain = self.app._normalize_paragraph(plain)
                pending = getattr(self.app, "_pending_assistant_normalized", None)
                targets = [value for value in (pending, self._last_assistant_message_normalized) if value]
                if self._suppress_console_duplicate and normalized_plain and targets:
                    if any(normalized_plain == target for target in targets):
                        continue
                if plain == self._last_console_line:
                    continue
                self._last_console_line = plain
            else:
                self._last_console_line = None

            if hasattr(self.app, "render_console_output"):
                self.app.render_console_output(renderable)
            else:
                self.app.conversation.write(renderable)
            has_rendered_content = True

        if not isinstance(renderables[-1], Text):
            self._last_console_line = None
        if self._suppress_console_duplicate:
            self._suppress_console_duplicate = False

        if hasattr(self.app, "stop_console_buffer"):
            self.app.stop_console_buffer()

    def set_suppress_duplicate(self, value: bool) -> None:
        """Set the suppress console duplicate flag.

        Args:
            value: Whether to suppress duplicates
        """
        self._suppress_console_duplicate = value

    def set_last_assistant_normalized(self, value: str | None) -> None:
        """Set the last assistant message normalized value.

        Args:
            value: The normalized assistant message
        """
        self._last_assistant_message_normalized = value


__all__ = ["ConsoleOutputManager"]
